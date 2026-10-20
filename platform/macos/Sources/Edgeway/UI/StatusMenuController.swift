import AppKit
import Combine

/// The menu bar item and its menu, modelled on the Wi-Fi and Bluetooth menus.
/// The menu is rebuilt only when it opens, so it costs nothing in between.
final class StatusMenuController: NSObject, NSMenuDelegate {
    private let model: AppModel
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let menu = NSMenu()
    private var cancellables: Set<AnyCancellable> = []

    var openSettings: ((SettingsTab) -> Void)?
    var openOnboarding: (() -> Void)?

    init(model: AppModel) {
        self.model = model
        super.init()
        menu.delegate = self
        menu.autoenablesItems = false
        statusItem.menu = menu
        statusItem.button?.setAccessibilityTitle("Edgeway")
        updateIcon()
        model.$snapshot
            .combineLatest(model.$accessibilityGranted)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.updateIcon() }
            .store(in: &cancellables)
    }

    private func updateIcon() {
        let s = model.snapshot
        let active = s.focus != nil || s.controlledBy != nil
        let hasConnection = !s.connectedDevices.isEmpty
        statusItem.button?.image = Brand.menuBarGlyph(active: active, paused: s.paused || !hasConnection)
        statusItem.button?.toolTip = "Edgeway — \(statusLine)"
    }

    // MARK: Menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        model.recheckPermissions()
        model.refresh()
        menu.removeAllItems()
        let s = model.snapshot

        menu.addItem(headerItem())
        if model.role == nil || model.needsOnboarding {
            menu.addItem(.separator())
            menu.addItem(action("Set Up Edgeway…", #selector(showOnboarding)))
            addFooter()
            return
        }
        if !model.accessibilityGranted {
            menu.addItem(.separator())
            let item = action("Allow Accessibility Access…", #selector(openAccessibility))
            item.image = NSImage(systemSymbolName: "exclamationmark.triangle.fill", accessibilityDescription: nil)
            menu.addItem(item)
        }

        menu.addItem(.separator())
        menu.addItem(sectionHeader(model.role == .primary ? "My Devices" : "Trusted Devices"))
        if s.devices.isEmpty {
            menu.addItem(note(model.role == .primary ? "No paired devices yet" : "Not paired with any Mac yet"))
        }
        for device in s.devices {
            menu.addItem(deviceItem(device))
        }

        if model.role == .primary {
            menu.addItem(.separator())
            menu.addItem(sectionHeader("Nearby"))
            if s.nearby.isEmpty {
                menu.addItem(note("Looking for computers running Edgeway…"))
            }
            for nearby in s.nearby {
                let item = action("Pair with \(nearby.instance)…", #selector(pair(_:)))
                item.representedObject = nearby.instance
                item.image = NSImage(systemSymbolName: nearby.platform == "macos" ? "desktopcomputer" : "pc", accessibilityDescription: nil)
                menu.addItem(item)
            }

            menu.addItem(.separator())
            let home = action("Return Control to This Mac", #selector(returnHome))
            home.keyEquivalent = "h"
            home.keyEquivalentModifierMask = [.control, .option, .command]
            home.isEnabled = s.focus != nil
            menu.addItem(home)
        } else {
            menu.addItem(.separator())
            menu.addItem(note("Pair from your other Mac: open Edgeway there and choose “\(s.name)” under Nearby."))
        }

        let pause = action(s.paused ? "Resume Sharing" : "Pause Sharing", #selector(togglePause))
        if model.role == .primary {
            pause.keyEquivalent = "p"
            pause.keyEquivalentModifierMask = [.control, .option, .command]
        } else {
            pause.title = s.paused ? "Allow Control" : "Pause Control"
        }
        menu.addItem(pause)

        menu.addItem(.separator())
        if model.role == .primary {
            menu.addItem(action("Arrange Displays…", #selector(showArrangement)))
        }
        addFooter()
    }

    private func addFooter() {
        let settings = action("Edgeway Settings…", #selector(showSettings))
        settings.keyEquivalent = ","
        menu.addItem(settings)
        menu.addItem(action("About Edgeway", #selector(showAbout)))
        menu.addItem(.separator())
        let quit = action("Quit Edgeway", #selector(quit))
        quit.keyEquivalent = "q"
        menu.addItem(quit)
    }

    private var statusLine: String {
        let s = model.snapshot
        if let error = model.engineError { return error }
        if model.role == nil { return "Not set up yet" }
        if s.paused { return model.role == .primary ? "Sharing paused" : "Control paused" }
        if model.role == .primary {
            if let focused = model.focusedDevice { return "Controlling \(focused.name)" }
            let connected = s.connectedDevices
            if connected.count == 1 { return "Connected to \(connected[0].name)" }
            if connected.count > 1 { return "Connected to \(connected.count) devices" }
            return s.devices.isEmpty ? "Ready to pair" : "Looking for your devices…"
        }
        if let controller = model.controllingDevice { return "Controlled by \(controller.name)" }
        if let connected = s.connectedDevices.first { return "Connected to \(connected.name)" }
        return "Waiting for a Mac to connect"
    }

    private func headerItem() -> NSMenuItem {
        let item = NSMenuItem()
        let title = NSMutableAttributedString(
            string: "Edgeway\n",
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.systemFontSize, weight: .semibold)]
        )
        title.append(NSAttributedString(
            string: statusLine,
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.secondaryLabelColor]
        ))
        item.attributedTitle = title
        item.isEnabled = false
        return item
    }

    private func deviceItem(_ device: DeviceRecord) -> NSMenuItem {
        let item = NSMenuItem()
        let title = NSMutableAttributedString(string: device.name + "\n")
        let isFocus = model.snapshot.focus == device.id || model.snapshot.controlledBy == device.id
        let detail = isFocus ? (model.role == .primary ? "Using keyboard and trackpad" : "Controlling this Mac") : device.statusText
        title.append(NSAttributedString(
            string: detail,
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.secondaryLabelColor]
        ))
        item.attributedTitle = title
        item.image = Brand.statusDot(device.isConnected ? .systemGreen : device.isConnecting ? .systemOrange : .tertiaryLabelColor)

        let submenu = NSMenu()
        let id = NSMenuItem(title: "ID \(device.shortId)", action: nil, keyEquivalent: "")
        id.isEnabled = false
        submenu.addItem(id)
        if let error = device.error {
            let e = NSMenuItem(title: error, action: nil, keyEquivalent: "")
            e.isEnabled = false
            submenu.addItem(e)
        }
        submenu.addItem(.separator())
        let forget = NSMenuItem(title: "Forget This Device…", action: #selector(forget(_:)), keyEquivalent: "")
        forget.target = self
        forget.representedObject = device
        submenu.addItem(forget)
        item.submenu = submenu
        return item
    }

    private func sectionHeader(_ title: String) -> NSMenuItem {
        if #available(macOS 14.0, *) {
            return NSMenuItem.sectionHeader(title: title)
        }
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.attributedTitle = NSAttributedString(
            string: title,
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold), .foregroundColor: NSColor.secondaryLabelColor]
        )
        item.isEnabled = false
        return item
    }

    private func note(_ text: String) -> NSMenuItem {
        let item = NSMenuItem()
        item.attributedTitle = NSAttributedString(
            string: text,
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.secondaryLabelColor]
        )
        item.isEnabled = false
        return item
    }

    private func action(_ title: String, _ selector: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = self
        return item
    }

    // MARK: Actions

    @objc private func pair(_ sender: NSMenuItem) {
        guard let instance = sender.representedObject as? String else { return }
        model.pair(with: instance)
    }

    @objc private func forget(_ sender: NSMenuItem) {
        guard let device = sender.representedObject as? DeviceRecord else { return }
        AppActivation.bringToFront()
        let alert = NSAlert()
        alert.messageText = "Forget “\(device.name)”?"
        alert.informativeText = "It will disconnect, and you’ll need to pair again with a new code to use it."
        alert.addButton(withTitle: "Forget Device")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        if alert.runModal() == .alertFirstButtonReturn {
            model.forget(device)
        }
    }

    @objc private func returnHome() { model.returnHome() }
    @objc private func togglePause() { model.togglePause() }
    @objc private func openAccessibility() {
        model.requestAccessibility()
        Permissions.openAccessibilitySettings()
    }
    @objc private func showSettings() { openSettings?(.general) }
    @objc private func showArrangement() { openSettings?(.arrangement) }
    @objc private func showOnboarding() { openOnboarding?() }

    @objc private func showAbout() {
        AppActivation.bringToFront()
        let credits = NSMutableAttributedString(
            string: "\(Brand.tagline)\n\nMade by \(Brand.author). Free and open source under the MIT License.",
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.secondaryLabelColor]
        )
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        credits.addAttribute(.paragraphStyle, value: paragraph, range: NSRange(location: 0, length: credits.length))
        NSApp.orderFrontStandardAboutPanel(options: [
            .credits: credits,
            .applicationIcon: Brand.appIcon,
        ])
    }

    @objc private func quit() { NSApp.terminate(nil) }
}
