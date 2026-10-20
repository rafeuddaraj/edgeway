import ServiceManagement

/// "Start Edgeway when you log in", backed by `SMAppService`.
///
/// This starts Edgeway after a user logs in. It cannot run before login: see
/// docs/LIMITATIONS.md for what that means after a restart and how to set up a
/// Mac mini so it reconnects on its own.
enum LaunchAtLogin {
    static var isEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// True when macOS wants the person to approve the login item in
    /// System Settings › General › Login Items.
    static var needsApproval: Bool {
        SMAppService.mainApp.status == .requiresApproval
    }

    @discardableResult
    static func set(_ enabled: Bool) -> Bool {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
            return true
        } catch {
            NSLog("Edgeway: could not change login item: \(error.localizedDescription)")
            return false
        }
    }

    static func openLoginItemsSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}
