import CEdgeway
import Foundation

/// Receives callbacks from the engine. Called on engine threads.
protocol EngineBridgeDelegate: AnyObject {
    func engineInject(_ event: EwEvent)
    func engineFreezeCursor(at point: CGPoint)
    func engineUnfreezeCursor(at point: CGPoint)
    func engineStateChanged()
    func enginePairing(_ event: PairingEvent)
}

enum PairingEvent: Equatable {
    case showCode(peer: String, code: String)
    case enterCode(peer: String)
    case succeeded(peer: String)
    case failed(peer: String, reason: String)
    case closed
}

/// Thin Swift wrapper over the C interface in `core/include/edgeway.h`.
final class EngineBridge {
    private var handle: OpaquePointer?
    private weak var delegate: EngineBridgeDelegate?
    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    static var version: String { String(cString: ew_version()) }

    init?(directory: URL, role: Role, name: String, screens: [DisplayInfo], delegate: EngineBridgeDelegate) {
        self.delegate = delegate
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

        let callbacks = EwCallbacks(
            context: Unmanaged.passUnretained(self).toOpaque(),
            inject: { ctx, event in
                guard let ctx, let event else { return }
                EngineBridge.from(ctx).delegate?.engineInject(event.pointee)
            },
            control: { ctx, op, x, y in
                guard let ctx else { return }
                let point = CGPoint(x: x, y: y)
                if op == UInt8(EW_CONTROL_FREEZE) {
                    EngineBridge.from(ctx).delegate?.engineFreezeCursor(at: point)
                } else {
                    EngineBridge.from(ctx).delegate?.engineUnfreezeCursor(at: point)
                }
            },
            state_changed: { ctx in
                guard let ctx else { return }
                EngineBridge.from(ctx).delegate?.engineStateChanged()
            },
            pairing: { ctx, kind, peer, detail in
                guard let ctx else { return }
                let peer = peer.map { String(cString: $0) } ?? ""
                let detail = detail.map { String(cString: $0) } ?? ""
                let event: PairingEvent
                switch Int32(kind) {
                case EW_PAIRING_SHOW_CODE: event = .showCode(peer: peer, code: detail)
                case EW_PAIRING_ENTER_CODE: event = .enterCode(peer: peer)
                case EW_PAIRING_SUCCEEDED: event = .succeeded(peer: peer)
                case EW_PAIRING_FAILED: event = .failed(peer: peer, reason: detail)
                default: event = .closed
                }
                EngineBridge.from(ctx).delegate?.enginePairing(event)
            }
        )

        let roleCode = UInt8(role == .primary ? EW_ROLE_PRIMARY : EW_ROLE_SECONDARY)
        handle = EngineBridge.withScreens(screens) { ptr, count in
            ew_engine_start(directory.path, roleCode, name, ptr, count, callbacks)
        }
        if handle == nil { return nil }
    }

    deinit {
        stop()
    }

    private static func from(_ ctx: UnsafeMutableRawPointer) -> EngineBridge {
        Unmanaged<EngineBridge>.fromOpaque(ctx).takeUnretainedValue()
    }

    private static func withScreens<T>(_ screens: [DisplayInfo], _ body: (UnsafePointer<EwScreen>?, Int) -> T) -> T {
        let names = screens.map { strdup($0.name) }
        defer { names.forEach { free($0) } }
        let raw = zip(screens, names).map { screen, name in
            EwScreen(x: screen.frame.minX, y: screen.frame.minY, w: screen.frame.width, h: screen.frame.height, name: name)
        }
        return raw.withUnsafeBufferPointer { body($0.baseAddress, $0.count) }
    }

    /// Stops the engine: says goodbye to peers and releases everything.
    func stop() {
        if let h = handle {
            handle = nil
            ew_engine_stop(h)
        }
    }

    /// Hot path from the event tap. Returns true to swallow the event.
    @inline(__always)
    func handleLocal(_ event: inout EwEvent) -> Bool {
        guard let h = handle else { return false }
        return ew_engine_local_event(h, &event) != 0
    }

    func setScreens(_ screens: [DisplayInfo]) {
        guard let h = handle else { return }
        EngineBridge.withScreens(screens) { ptr, count in ew_engine_set_screens(h, ptr, count) }
    }

    @discardableResult
    func command(_ verb: String, _ argument: String = "") -> Bool {
        guard let h = handle else { return false }
        return ew_engine_command(h, verb, argument) != 0
    }

    func snapshot() -> Snapshot? {
        guard let h = handle, let raw = ew_engine_snapshot(h) else { return nil }
        defer { ew_string_free(raw) }
        let data = Data(bytes: raw, count: strlen(raw))
        return try? decoder.decode(Snapshot.self, from: data)
    }
}
