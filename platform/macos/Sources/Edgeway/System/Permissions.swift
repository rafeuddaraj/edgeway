import AppKit
import ApplicationServices

/// The one permission Edgeway needs on macOS: Accessibility. It allows the
/// sharing Mac to intercept its own keyboard and trackpad while another
/// computer has focus, and the controlled Mac to post the input it receives.
enum Permissions {
    static var isAccessibilityTrusted: Bool {
        AXIsProcessTrusted()
    }

    /// Adds Edgeway to the Accessibility list and shows the system prompt.
    static func requestAccessibility() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        _ = AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    static func openAccessibilitySettings() {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        NSWorkspace.shared.open(url)
    }

    static func openLocalNetworkSettings() {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork")!
        NSWorkspace.shared.open(url)
    }

    /// Calls `handler` on the main thread whenever the Accessibility list
    /// changes. macOS posts a distributed notification for this, so there is
    /// no need to poll.
    static func observeAccessibilityChanges(_ handler: @escaping () -> Void) -> NSObjectProtocol {
        DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("com.apple.accessibility.api"),
            object: nil,
            queue: .main
        ) { _ in
            // The trust database updates just after the notification.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: handler)
        }
    }
}
