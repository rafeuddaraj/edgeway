import AppKit
import Network

/// Sleep, wake, screen lock, display and network changes, turned into a few
/// simple callbacks. Everything is notification-driven; nothing polls.
final class SystemEvents {
    var onWake: (() -> Void)?
    var onSleepOrLock: (() -> Void)?
    var onDisplaysChanged: (() -> Void)?
    var onNetworkChanged: (() -> Void)?

    private var observers: [(NotificationCenter, NSObjectProtocol)] = []
    private let pathMonitor = NWPathMonitor()
    private var lastPath: String?

    func start() {
        let workspace = NSWorkspace.shared.notificationCenter
        let distributed = DistributedNotificationCenter.default()
        observe(workspace, NSWorkspace.didWakeNotification) { [weak self] in self?.onWake?() }
        observe(workspace, NSWorkspace.screensDidWakeNotification) { [weak self] in self?.onWake?() }
        observe(workspace, NSWorkspace.willSleepNotification) { [weak self] in self?.onSleepOrLock?() }
        observe(workspace, NSWorkspace.sessionDidResignActiveNotification) { [weak self] in self?.onSleepOrLock?() }
        observe(distributed, Notification.Name("com.apple.screenIsLocked")) { [weak self] in self?.onSleepOrLock?() }
        observe(NotificationCenter.default, NSApplication.didChangeScreenParametersNotification) { [weak self] in
            self?.onDisplaysChanged?()
        }

        pathMonitor.pathUpdateHandler = { [weak self] path in
            let summary = "\(path.status)|" + path.availableInterfaces.map(\.name).joined(separator: ",")
            DispatchQueue.main.async {
                guard let self, self.lastPath != summary else { return }
                let isFirst = self.lastPath == nil
                self.lastPath = summary
                if !isFirst, path.status == .satisfied { self.onNetworkChanged?() }
            }
        }
        pathMonitor.start(queue: DispatchQueue(label: "edgeway.network-path"))
    }

    func stop() {
        pathMonitor.cancel()
        for (center, token) in observers { center.removeObserver(token) }
        observers.removeAll()
    }

    private func observe(_ center: NotificationCenter, _ name: Notification.Name, _ handler: @escaping () -> Void) {
        let token = center.addObserver(forName: name, object: nil, queue: .main) { _ in handler() }
        observers.append((center, token))
    }
}
