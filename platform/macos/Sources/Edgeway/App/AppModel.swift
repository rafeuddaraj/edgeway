import AppKit
import CEdgeway
import Combine
import SystemConfiguration

/// Pairing as the UI presents it.
enum PairingState: Equatable {
    /// This Mac shows a code (it is being paired with).
    case showingCode(peer: String, code: String)
    /// The person types the code shown on `peer`.
    case enteringCode(peer: String)
    case working(peer: String)
    case succeeded(peer: String)
    case failed(peer: String, reason: String)
}

/// Owns the engine and the platform pieces around it, and exposes the state
/// SwiftUI and the menu bar render. Lives on the main thread; engine callbacks
/// hop here before touching published state.
final class AppModel: ObservableObject {
    @Published private(set) var snapshot: Snapshot = .empty
    @Published private(set) var role: Role?
    @Published private(set) var accessibilityGranted = Permissions.isAccessibilityTrusted
    @Published private(set) var launchAtLogin = LaunchAtLogin.isEnabled
    @Published private(set) var launchAtLoginNeedsApproval = LaunchAtLogin.needsApproval
    @Published private(set) var pairing: PairingState?
    @Published private(set) var engineError: String?
    /// Short notice shown by the HUD (e.g. secure input took control back).
    let notices = PassthroughSubject<(String, String), Never>()

    let options: LaunchOptions
    let deviceName: String
    private let defaults: UserDefaults

    private var engine: EngineBridge?
    private let cursor = CursorController()
    private var tap: EventTap?
    private var injector: EventInjector?
    private var secureMonitor: SecureInputMonitor?
    private var hotkeys: Hotkeys?
    private let system = SystemEvents()
    private var accessibilityObserver: NSObjectProtocol?
    private var refreshScheduled = false
    private let refreshLock = NSLock()
    private var lastPairTarget: String?

    init(options: LaunchOptions) {
        self.options = options
        defaults = options.defaults
        let computerName = (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? Host.current().localizedName ?? "Mac"
        deviceName = options.profile.map { "\(computerName) (\($0))" } ?? computerName
        role = options.role ?? defaults.string(forKey: "role").flatMap(Role.init(rawValue:))
    }

    var needsOnboarding: Bool { role == nil || !defaults.bool(forKey: "onboarded") }

    var focusedDevice: DeviceRecord? { snapshot.device(id: snapshot.focus) }
    var controllingDevice: DeviceRecord? { snapshot.device(id: snapshot.controlledBy) }

    // MARK: Lifecycle

    func start() {
        accessibilityObserver = Permissions.observeAccessibilityChanges { [weak self] in
            self?.permissionsMayHaveChanged()
        }
        system.onWake = { [weak self] in self?.engine?.command("network-changed") }
        system.onNetworkChanged = { [weak self] in self?.engine?.command("network-changed") }
        system.onSleepOrLock = { [weak self] in self?.engine?.command("home") }
        system.onDisplaysChanged = { [weak self] in self?.engine?.setScreens(DisplayInfo.current()) }
        system.start()
        if role != nil { startEngine() }
    }

    func shutdown() {
        stopEngine()
        system.stop()
        cursor.restore()
    }

    private func startEngine() {
        stopEngine()
        guard let role else { return }
        guard let bridge = EngineBridge(
            directory: options.dataDirectory,
            role: role,
            name: deviceName,
            screens: DisplayInfo.current(),
            delegate: self
        ) else {
            engineError = "Edgeway couldn’t start. Check that \(options.dataDirectory.path) is writable."
            return
        }
        engineError = nil
        engine = bridge
        switch role {
        case .primary:
            secureMonitor = SecureInputMonitor { [weak self] in self?.secureInputDetected() }
            hotkeys = Hotkeys { [weak self] action in
                switch action {
                case .returnHome: self?.returnHome()
                case .togglePause: self?.togglePause()
                }
            }
            hotkeys?.register()
            startTapIfPossible()
        case .secondary:
            injector = EventInjector(dryRun: options.dryRun)
        }
        refresh()
    }

    private func stopEngine() {
        tap?.stop()
        tap = nil
        hotkeys?.unregister()
        hotkeys = nil
        secureMonitor?.stop()
        secureMonitor = nil
        engine?.stop()
        engine = nil
        injector = nil
        cursor.restore()
        pairing = nil
        snapshot = .empty
    }

    private func startTapIfPossible() {
        guard role == .primary, tap == nil, accessibilityGranted, let engine else { return }
        let tap = EventTap { event in engine.handleLocal(&event) }
        tap.onInterrupted = { [weak self] in
            // macOS paused the tap; anything typed meanwhile went to this Mac.
            DispatchQueue.main.async { self?.engine?.command("home") }
        }
        if tap.start() {
            self.tap = tap
        }
    }

    private func permissionsMayHaveChanged() {
        let granted = Permissions.isAccessibilityTrusted
        guard granted != accessibilityGranted else { return }
        accessibilityGranted = granted
        if granted {
            startTapIfPossible()
        } else {
            tap?.stop()
            tap = nil
            cursor.restore()
        }
    }

    func recheckPermissions() {
        permissionsMayHaveChanged()
        launchAtLogin = LaunchAtLogin.isEnabled
        launchAtLoginNeedsApproval = LaunchAtLogin.needsApproval
    }

    // MARK: Settings

    func setRole(_ newRole: Role) {
        guard newRole != role else { return }
        role = newRole
        defaults.set(newRole.rawValue, forKey: "role")
        startEngine()
    }

    func finishOnboarding() {
        defaults.set(true, forKey: "onboarded")
        objectWillChange.send()
    }

    func setLaunchAtLogin(_ enabled: Bool) {
        LaunchAtLogin.set(enabled)
        launchAtLogin = LaunchAtLogin.isEnabled
        launchAtLoginNeedsApproval = LaunchAtLogin.needsApproval
    }

    func requestAccessibility() {
        Permissions.requestAccessibility()
    }

    func setEdgeResistance(_ points: Double) {
        engine?.command("resistance", String(Int(points)))
    }

    // MARK: Actions

    func togglePause() {
        engine?.command(snapshot.paused ? "resume" : "pause")
    }

    func returnHome() {
        engine?.command("home")
    }

    func pair(with instance: String) {
        lastPairTarget = instance
        pairing = .working(peer: instance)
        if engine?.command("pair", instance) != true {
            pairing = .failed(peer: instance, reason: "\(instance) is no longer available. Make sure Edgeway is open on it.")
        }
    }

    func retryPairing() {
        guard let target = lastPairTarget else { return }
        pair(with: target)
    }

    func submitCode(_ code: String) {
        if engine?.command("code", code) == true, case .enteringCode(let peer) = pairing {
            pairing = .working(peer: peer)
        }
    }

    func cancelPairing() {
        engine?.command("cancel-pairing")
        pairing = nil
    }

    func dismissPairing() {
        pairing = nil
    }

    func forget(_ device: DeviceRecord) {
        engine?.command("forget", device.id)
    }

    func place(_ device: DeviceRecord, at origin: CGPoint) {
        engine?.command("place", "\(device.id) \(Int(origin.x.rounded())) \(Int(origin.y.rounded()))")
    }

    // MARK: Engine state

    /// Coalesces bursts of engine notifications into one main-thread refresh.
    private func scheduleRefresh() {
        refreshLock.lock()
        let already = refreshScheduled
        refreshScheduled = true
        refreshLock.unlock()
        guard !already else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.refreshLock.lock()
            self.refreshScheduled = false
            self.refreshLock.unlock()
            self.refresh()
        }
    }

    func refresh() {
        guard let engine, let next = engine.snapshot() else { return }
        if next != snapshot { snapshot = next }
    }

    private func secureInputDetected() {
        guard cursor.isAway else { return }
        engine?.command("home", "secure-input")
        notices.send(("Control returned to this Mac", "A password field or secure app turned on Secure Input."))
    }
}

extension AppModel: EngineBridgeDelegate {
    func engineInject(_ event: EwEvent) {
        injector?.post(event)
    }

    func engineFreezeCursor(at point: CGPoint) {
        cursor.freeze(at: point)
        DispatchQueue.main.async { [weak self] in self?.secureMonitor?.start() }
    }

    func engineUnfreezeCursor(at point: CGPoint) {
        cursor.unfreeze(at: point)
        DispatchQueue.main.async { [weak self] in self?.secureMonitor?.stop() }
    }

    func engineStateChanged() {
        scheduleRefresh()
    }

    func enginePairing(_ event: PairingEvent) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            switch event {
            case .showCode(let peer, let code): self.pairing = .showingCode(peer: peer, code: code)
            case .enterCode(let peer): self.pairing = .enteringCode(peer: peer)
            case .succeeded(let peer): self.pairing = .succeeded(peer: peer)
            case .failed(let peer, let reason): self.pairing = .failed(peer: peer, reason: reason)
            case .closed: self.pairing = nil
            }
            self.refresh()
        }
    }
}

#if EDGEWAY_PREVIEWS
extension AppModel {
    /// Loads sample state for `scripts/render-previews.sh`.
    func loadPreview(snapshot: Snapshot, role: Role, pairing: PairingState?, accessibilityGranted: Bool) {
        self.snapshot = snapshot
        self.role = role
        self.pairing = pairing
        self.accessibilityGranted = accessibilityGranted
    }
}
#endif
