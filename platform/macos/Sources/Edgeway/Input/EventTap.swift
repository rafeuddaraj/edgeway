import CEdgeway
import CoreGraphics
import Foundation

/// Marks events Edgeway posts itself, so a tap never re-captures them.
let edgewayEventMarker: Int64 = 0x4544_4757 // "EDGW"

/// Captures every keyboard, pointer and scroll event on the sharing Mac and
/// asks the engine whether it belongs here or on another device.
///
/// The tap runs on its own high-priority thread with its own run loop, so a
/// busy main thread can never delay input or make macOS disable the tap.
final class EventTap {
    /// Returns true to swallow the event.
    private let route: (inout EwEvent) -> Bool
    /// The tap was disabled by macOS (timeout or user input) and re-enabled.
    var onInterrupted: (() -> Void)?

    private var port: CFMachPort?
    private var runLoop: CFRunLoop?
    private var thread: Thread?

    init(route: @escaping (inout EwEvent) -> Bool) {
        self.route = route
    }

    var isRunning: Bool { port != nil }

    /// Event types of interest, including trackpad gestures (raw NSEvent
    /// types) so pinches and swipes never leak to this Mac while away.
    private static let mask: CGEventMask = {
        let types: [UInt32] = [
            1, 2, 3, 4, 5, 6, 7, // left/right down/up, moved, dragged
            10, 11, 12, // key down/up, flags changed
            14, // system defined (media keys)
            18, 19, 20, // rotate, begin/end gesture
            22, // scroll wheel
            25, 26, 27, // other mouse down/up/dragged
            29, 30, 31, 32, 34, // gesture, magnify, swipe, smart magnify, pressure
        ]
        return types.reduce(CGEventMask(0)) { $0 | (CGEventMask(1) << CGEventMask($1)) }
    }()

    /// Starts capturing. Fails without the Accessibility permission.
    func start() -> Bool {
        guard port == nil else { return true }
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: EventTap.mask,
            callback: EventTap.callback,
            userInfo: refcon
        ) else { return false }
        port = tap

        let ready = DispatchSemaphore(value: 0)
        let thread = Thread { [weak self] in
            let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
            CGEvent.tapEnable(tap: tap, enable: true)
            self?.runLoop = CFRunLoopGetCurrent()
            ready.signal()
            CFRunLoopRun()
        }
        thread.name = "Edgeway Event Tap"
        thread.qualityOfService = .userInteractive
        thread.start()
        self.thread = thread
        ready.wait()
        return true
    }

    func stop() {
        guard let tap = port else { return }
        CGEvent.tapEnable(tap: tap, enable: false)
        CFMachPortInvalidate(tap)
        if let rl = runLoop { CFRunLoopStop(rl) }
        port = nil
        runLoop = nil
        thread = nil
    }

    private static let callback: CGEventTapCallBack = { _, type, event, refcon in
        guard let refcon else { return Unmanaged.passUnretained(event) }
        let tap = Unmanaged<EventTap>.fromOpaque(refcon).takeUnretainedValue()
        return tap.handle(type: type, event: event)
    }

    private func handle(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
            if let tap = port { CGEvent.tapEnable(tap: tap, enable: true) }
            onInterrupted?()
            return Unmanaged.passUnretained(event)
        }
        if event.getIntegerValueField(.eventSourceUserData) == edgewayEventMarker {
            return Unmanaged.passUnretained(event)
        }
        var swallow = false
        for var ev in EventTap.translate(type: type, event: event) {
            swallow = route(&ev) || swallow
        }
        return swallow ? nil : Unmanaged.passUnretained(event)
    }

    /// Converts a Core Graphics event into zero or more neutral events.
    static func translate(type: CGEventType, event: CGEvent) -> [EwEvent] {
        var ev = EwEvent()
        switch type.rawValue {
        case 5, 6, 7, 27: // moved and dragged
            ev.kind = UInt8(EW_EVENT_MOUSE_MOVE)
            ev.x = event.location.x
            ev.y = event.location.y
            ev.dx = event.getDoubleValueField(.mouseEventDeltaX)
            ev.dy = event.getDoubleValueField(.mouseEventDeltaY)
        case 1, 2, 3, 4, 25, 26:
            ev.kind = UInt8(EW_EVENT_BUTTON)
            switch type.rawValue {
            case 1, 2: ev.button = 0
            case 3, 4: ev.button = 1
            default: ev.button = UInt8(clamping: event.getIntegerValueField(.mouseEventButtonNumber))
            }
            ev.down = [1, 3, 25].contains(type.rawValue) ? 1 : 0
            ev.clicks = UInt8(clamping: event.getIntegerValueField(.mouseEventClickState))
        case 22:
            ev.kind = UInt8(EW_EVENT_SCROLL)
            let continuous = event.getIntegerValueField(.scrollWheelEventIsContinuous) != 0
            ev.continuous = continuous ? 1 : 0
            ev.dy = event.getDoubleValueField(.scrollWheelEventPointDeltaAxis1)
            ev.dx = event.getDoubleValueField(.scrollWheelEventPointDeltaAxis2)
            ev.lines_y = Int16(clamping: event.getIntegerValueField(.scrollWheelEventDeltaAxis1))
            ev.lines_x = Int16(clamping: event.getIntegerValueField(.scrollWheelEventDeltaAxis2))
            ev.phase = ScrollPhase.toNeutral(event.getIntegerValueField(.scrollWheelEventScrollPhase))
            ev.momentum = ScrollPhase.momentumToNeutral(event.getIntegerValueField(.scrollWheelEventMomentumPhase))
        case 10, 11:
            ev.kind = UInt8(EW_EVENT_KEY)
            ev.key = ew_key_from_mac(UInt16(clamping: event.getIntegerValueField(.keyboardEventKeycode)))
            ev.down = type.rawValue == 10 ? 1 : 0
            ev.`repeat` = event.getIntegerValueField(.keyboardEventAutorepeat) != 0 ? 1 : 0
        case 12:
            // Modifiers arrive as "flags changed": the key's own device bit
            // tells us whether it went down or up.
            let key = ew_key_from_mac(UInt16(clamping: event.getIntegerValueField(.keyboardEventKeycode)))
            guard key != 0 else { return [EwEvent(kind: UInt8(EW_EVENT_OTHER))] }
            ev.kind = UInt8(EW_EVENT_KEY)
            ev.key = key
            if key == 0x39 {
                // Caps Lock reports one event per press: forward a full tap.
                var up = ev
                ev.down = 1
                up.down = 0
                return [ev, up]
            }
            let bits = ew_mac_flags_for_key(key)
            let deviceBits = bits & 0xFFFF
            let flags = event.flags.rawValue
            ev.down = (deviceBits != 0 ? flags & deviceBits != 0 : flags & bits != 0) ? 1 : 0
        default:
            ev.kind = UInt8(EW_EVENT_OTHER)
        }
        return [ev]
    }
}

private extension EwEvent {
    init(kind: UInt8) {
        self.init()
        self.kind = kind
    }
}

/// Scroll phases: Core Graphics values ↔ Edgeway's neutral values.
enum ScrollPhase {
    static func toNeutral(_ cg: Int64) -> UInt8 {
        switch cg {
        case 1: return 1 // began
        case 2: return 2 // changed
        case 4: return 3 // ended
        case 8: return 4 // cancelled
        case 128: return 5 // may begin
        default: return 0
        }
    }

    static func fromNeutral(_ v: UInt8) -> Int64 {
        switch v {
        case 1: return 1
        case 2: return 2
        case 3: return 4
        case 4: return 8
        case 5: return 128
        default: return 0
        }
    }

    static func momentumToNeutral(_ cg: Int64) -> UInt8 {
        switch cg {
        case 1: return 1 // begin
        case 2: return 2 // continue
        case 3: return 3 // end
        default: return 0
        }
    }

    static func momentumFromNeutral(_ v: UInt8) -> Int64 {
        Int64(min(v, 3))
    }
}
