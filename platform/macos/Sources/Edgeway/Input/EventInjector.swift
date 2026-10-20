import CEdgeway
import CoreGraphics
import Foundation
import IOKit.hidsystem
import IOKit.pwr_mgt

/// Posts the input received from the controlling device into this Mac's
/// session. Every event is tagged with ``edgewayEventMarker`` so an event tap
/// on the same Mac ignores it.
final class EventInjector {
    private let dryRun: Bool
    private let source = CGEventSource(stateID: .hidSystemState)
    private var activityAssertion: IOPMAssertionID = 0
    private let queue = DispatchQueue(label: "edgeway.inject", qos: .userInteractive)

    init(dryRun: Bool) {
        self.dryRun = dryRun
        // Pointer movement must not be suppressed after our own warps.
        source?.localEventsSuppressionInterval = 0
    }

    /// Engine threads call this; posting is serialised on one queue so
    /// ordering is preserved.
    func post(_ ev: EwEvent) {
        queue.async { [self] in
            if dryRun {
                FileHandle.standardError.write(Data("[edgeway] inject \(describe(ev))\n".utf8))
                return
            }
            perform(ev)
        }
    }

    private func perform(_ ev: EwEvent) {
        let flags = CGEventFlags(rawValue: ew_mac_flags_from_mods(ev.mods))
        let point = CGPoint(x: ev.x, y: ev.y)
        var event: CGEvent?

        switch Int32(ev.kind) {
        case EW_EVENT_MOUSE_MOVE:
            let (type, button): (CGEventType, CGMouseButton) =
                ev.buttons & 1 != 0 ? (.leftMouseDragged, .left)
                : ev.buttons & 2 != 0 ? (.rightMouseDragged, .right)
                : ev.buttons != 0 ? (.otherMouseDragged, .center)
                : (.mouseMoved, .left)
            event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: button)
            event?.setIntegerValueField(.mouseEventDeltaX, value: Int64(ev.dx.rounded()))
            event?.setIntegerValueField(.mouseEventDeltaY, value: Int64(ev.dy.rounded()))
        case EW_EVENT_BUTTON:
            let down = ev.down != 0
            let (type, button): (CGEventType, CGMouseButton) = switch ev.button {
            case 0: (down ? .leftMouseDown : .leftMouseUp, .left)
            case 1: (down ? .rightMouseDown : .rightMouseUp, .right)
            default: (down ? .otherMouseDown : .otherMouseUp, CGMouseButton(rawValue: UInt32(ev.button)) ?? .center)
            }
            event = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: button)
            event?.setIntegerValueField(.mouseEventClickState, value: Int64(max(1, ev.clicks)))
            if ev.button > 1 { event?.setIntegerValueField(.mouseEventButtonNumber, value: Int64(ev.button)) }
        case EW_EVENT_SCROLL:
            let continuous = ev.continuous != 0
            event = CGEvent(
                scrollWheelEvent2Source: source,
                units: continuous ? .pixel : .line,
                wheelCount: 2,
                wheel1: Int32(continuous ? ev.dy.rounded() : Double(ev.lines_y)),
                wheel2: Int32(continuous ? ev.dx.rounded() : Double(ev.lines_x)),
                wheel3: 0
            )
            if let e = event {
                e.setIntegerValueField(.scrollWheelEventIsContinuous, value: continuous ? 1 : 0)
                e.setIntegerValueField(.scrollWheelEventPointDeltaAxis1, value: Int64(ev.dy.rounded()))
                e.setIntegerValueField(.scrollWheelEventPointDeltaAxis2, value: Int64(ev.dx.rounded()))
                e.setIntegerValueField(.scrollWheelEventScrollPhase, value: ScrollPhase.fromNeutral(ev.phase))
                e.setIntegerValueField(.scrollWheelEventMomentumPhase, value: ScrollPhase.momentumFromNeutral(ev.momentum))
            }
        case EW_EVENT_KEY:
            let code = ew_key_to_mac(ev.key)
            guard code >= 0 else { return }
            if ev.key == 0x39 {
                if ev.down != 0 { toggleCapsLock() }
                return
            }
            event = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(code), keyDown: ev.down != 0)
            let modifierBits = ew_mac_flags_for_key(ev.key)
            if modifierBits != 0 {
                event?.type = .flagsChanged
                let deviceBits = ev.down != 0 ? modifierBits & 0xFFFF : 0
                event?.flags = CGEventFlags(rawValue: flags.rawValue | deviceBits)
            } else {
                event?.flags = flags
                event?.setIntegerValueField(.keyboardEventAutorepeat, value: ev.`repeat` != 0 ? 1 : 0)
            }
        case EW_EVENT_WAKE:
            declareUserActivity()
            return
        default:
            return
        }

        guard let event else { return }
        if Int32(ev.kind) != EW_EVENT_KEY { event.flags = flags }
        event.setIntegerValueField(.eventSourceUserData, value: edgewayEventMarker)
        event.post(tap: .cghidEventTap)
    }

    /// Caps Lock is a lock state, not a key press, on macOS.
    private func toggleCapsLock() {
        let service = IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching(kIOHIDSystemClass))
        guard service != 0 else { return }
        defer { IOObjectRelease(service) }
        var connect: io_connect_t = 0
        guard IOServiceOpen(service, mach_task_self_, UInt32(kIOHIDParamConnectType), &connect) == KERN_SUCCESS else { return }
        defer { IOServiceClose(connect) }
        var state = false
        IOHIDGetModifierLockState(connect, Int32(kIOHIDCapsLockState), &state)
        IOHIDSetModifierLockState(connect, Int32(kIOHIDCapsLockState), !state)
    }

    /// Wakes the display and resets the idle timer, as real input would.
    private func declareUserActivity() {
        IOPMAssertionDeclareUserActivity("Edgeway remote input" as CFString, kIOPMUserActiveLocal, &activityAssertion)
    }

    private func describe(_ ev: EwEvent) -> String {
        switch Int32(ev.kind) {
        case EW_EVENT_MOUSE_MOVE: return "move \(Int(ev.x)),\(Int(ev.y)) buttons=\(ev.buttons)"
        case EW_EVENT_BUTTON: return "button \(ev.button) \(ev.down != 0 ? "down" : "up") clicks=\(ev.clicks)"
        case EW_EVENT_SCROLL: return "scroll \(ev.dx),\(ev.dy) phase=\(ev.phase) momentum=\(ev.momentum)"
        case EW_EVENT_KEY: return "key 0x\(String(ev.key, radix: 16)) \(ev.down != 0 ? "down" : "up") mods=\(ev.mods)"
        case EW_EVENT_WAKE: return "wake"
        default: return "other"
        }
    }
}
