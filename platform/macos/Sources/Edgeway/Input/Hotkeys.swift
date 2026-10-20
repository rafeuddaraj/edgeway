import Carbon.HIToolbox
import Foundation

/// System-wide shortcuts. Carbon hot keys keep working while another app
/// holds Secure Event Input, when an event tap would see nothing:
///
/// * ⌃⌥⌘H — bring control back to this Mac
/// * ⌃⌥⌘P — pause or resume sharing
final class Hotkeys {
    enum Action: UInt32 {
        case returnHome = 1
        case togglePause = 2
    }

    private var refs: [EventHotKeyRef] = []
    private var handlerRef: EventHandlerRef?
    private let onAction: (Action) -> Void

    init(onAction: @escaping (Action) -> Void) {
        self.onAction = onAction
    }

    func register() {
        guard handlerRef == nil else { return }
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, event, refcon in
            guard let event, let refcon else { return OSStatus(eventNotHandledErr) }
            var id = EventHotKeyID()
            GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil,
                              MemoryLayout<EventHotKeyID>.size, nil, &id)
            let me = Unmanaged<Hotkeys>.fromOpaque(refcon).takeUnretainedValue()
            if let action = Action(rawValue: id.id) {
                DispatchQueue.main.async { me.onAction(action) }
            }
            return noErr
        }, 1, &spec, refcon, &handlerRef)

        let modifiers = UInt32(controlKey | optionKey | cmdKey)
        add(.returnHome, key: UInt32(kVK_ANSI_H), modifiers: modifiers)
        add(.togglePause, key: UInt32(kVK_ANSI_P), modifiers: modifiers)
    }

    func unregister() {
        refs.forEach { UnregisterEventHotKey($0) }
        refs.removeAll()
        if let h = handlerRef { RemoveEventHandler(h) }
        handlerRef = nil
    }

    private func add(_ action: Action, key: UInt32, modifiers: UInt32) {
        var ref: EventHotKeyRef?
        let id = EventHotKeyID(signature: OSType(0x4544_4757), id: action.rawValue)
        if RegisterEventHotKey(key, modifiers, id, GetApplicationEventTarget(), 0, &ref) == noErr, let ref {
            refs.append(ref)
        }
    }
}

/// While another computer has focus, watches for Secure Event Input. When a
/// password field (or a terminal's Secure Keyboard Entry) turns it on, macOS
/// stops delivering keystrokes to every event tap — they would land in that
/// field on this Mac. Edgeway brings control home instead of letting that
/// happen silently.
final class SecureInputMonitor {
    private var timer: DispatchSourceTimer?
    private let onSecureInput: () -> Void

    init(onSecureInput: @escaping () -> Void) {
        self.onSecureInput = onSecureInput
    }

    static var isActive: Bool { IsSecureEventInputEnabled() }

    /// Checks four times a second, only while away.
    func start() {
        guard timer == nil else { return }
        let t = DispatchSource.makeTimerSource(queue: .main)
        t.schedule(deadline: .now(), repeating: .milliseconds(250), leeway: .milliseconds(50))
        t.setEventHandler { [weak self] in
            if IsSecureEventInputEnabled() { self?.onSecureInput() }
        }
        t.resume()
        timer = t
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }
}
