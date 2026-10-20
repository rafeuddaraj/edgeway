import AppKit
import Combine

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let model: AppModel
    private var statusMenu: StatusMenuController?
    private var settings: SettingsWindowController?
    private var onboarding: OnboardingWindowController?
    private var pairing: PairingWindowController?
    private let hud = NoticeHUD()
    private var cancellables: Set<AnyCancellable> = []

    init(options: LaunchOptions) {
        model = AppModel(options: options)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        TerminationSignals.install()
        statusMenu = StatusMenuController(model: model)
        statusMenu?.openSettings = { [weak self] tab in self?.showSettings(tab) }
        statusMenu?.openOnboarding = { [weak self] in self?.showOnboarding() }
        pairing = PairingWindowController(model: model)
        model.notices
            .receive(on: RunLoop.main)
            .sink { [weak self] title, message in self?.hud.show(title: title, message: message) }
            .store(in: &cancellables)

        model.start()
        if model.needsOnboarding { showOnboarding() }
    }

    func applicationWillTerminate(_ notification: Notification) {
        model.shutdown()
    }

    /// Opening the app again from Finder or Spotlight shows its settings.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if model.needsOnboarding { showOnboarding() } else { showSettings(.general) }
        return false
    }

    private func showSettings(_ tab: SettingsTab) {
        if settings == nil {
            let controller = SettingsWindowController(model: model)
            controller.onClose = { [weak self] in
                DispatchQueue.main.async { self?.settings = nil }
            }
            settings = controller
        }
        settings?.show(tab)
    }

    private func showOnboarding() {
        if onboarding == nil {
            let controller = OnboardingWindowController(model: model)
            controller.onClose = { [weak self] in
                DispatchQueue.main.async { self?.onboarding = nil }
            }
            onboarding = controller
        }
        onboarding?.show()
    }
}

/// Bringing a menu bar app forward when it opens a window.
enum AppActivation {
    static func bringToFront() {
        if #available(macOS 14.0, *) {
            NSApp.activate()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
    }
}

/// Quits cleanly on SIGTERM/SIGINT (e.g. `kill`, logout scripts), so peers get
/// a goodbye, held keys are released and the cursor is restored.
enum TerminationSignals {
    private static var sources: [DispatchSourceSignal] = []

    static func install() {
        for sig in [SIGTERM, SIGINT] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { NSApp.terminate(nil) }
            source.resume()
            sources.append(source)
        }
    }
}
