import AppKit
import SwiftUI

enum SettingsTab: Int, CaseIterable {
    case general, devices, arrangement

    var title: String {
        switch self {
        case .general: return "General"
        case .devices: return "Devices"
        case .arrangement: return "Arrangement"
        }
    }

    var symbol: String {
        switch self {
        case .general: return "gearshape"
        case .devices: return "laptopcomputer.and.arrow.down"
        case .arrangement: return "rectangle.on.rectangle"
        }
    }
}

/// A standard macOS settings window with toolbar tabs. Created when first
/// opened and released when closed, so it costs no memory in between.
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    private let tabs = NSTabViewController()
    var onClose: (() -> Void)?

    init(model: AppModel) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 620, height: 460),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: true
        )
        window.isReleasedWhenClosed = false
        window.toolbarStyle = .preference
        super.init(window: window)
        window.delegate = self

        tabs.tabStyle = .toolbar
        tabs.transitionOptions = [.allowUserInteraction]
        for tab in SettingsTab.allCases {
            let item = NSTabViewItem(viewController: host(tab, model: model))
            item.label = tab.title
            item.image = NSImage(systemSymbolName: tab.symbol, accessibilityDescription: tab.title)
            tabs.addTabViewItem(item)
        }
        window.contentViewController = tabs
        window.title = SettingsTab.general.title
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private func host(_ tab: SettingsTab, model: AppModel) -> NSViewController {
        let view: AnyView
        switch tab {
        case .general: view = AnyView(GeneralSettingsView().environmentObject(model))
        case .devices: view = AnyView(DevicesSettingsView().environmentObject(model))
        case .arrangement: view = AnyView(ArrangementSettingsView().environmentObject(model))
        }
        let controller = NSHostingController(rootView: view)
        controller.sizingOptions = [.preferredContentSize]
        return controller
    }

    func show(_ tab: SettingsTab) {
        tabs.selectedTabViewItemIndex = tab.rawValue
        window?.title = tab.title
        if window?.isVisible != true { window?.center() }
        AppActivation.bringToFront()
        window?.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        onClose?()
    }
}

// MARK: - General

struct GeneralSettingsView: View {
    @EnvironmentObject private var model: AppModel

    private static let resistances: [(String, Double)] = [("None", 0), ("Light", 15), ("Medium", 40), ("Firm", 90)]

    var body: some View {
        Form {
            Section {
                Picker("This Mac", selection: Binding(get: { model.role ?? .primary }, set: { model.setRole($0) })) {
                    Text("Shares its keyboard and trackpad").tag(Role.primary)
                    Text("Is controlled by another computer").tag(Role.secondary)
                }
                .pickerStyle(.radioGroup)
                LabeledContent("Name") { Text(model.snapshot.name.isEmpty ? model.deviceName : model.snapshot.name) }
                LabeledContent("Device ID") {
                    Text(model.snapshot.shortId).font(.body.monospaced()).textSelection(.enabled)
                }
            } footer: {
                Text("Other computers see this name. Change it in System Settings › General › About.")
                    .settingsFootnote()
            }

            Section {
                Toggle("Start Edgeway when you log in", isOn: Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) }))
                if model.launchAtLoginNeedsApproval {
                    HStack {
                        Text("Approve Edgeway in Login Items to finish.").foregroundStyle(.secondary)
                        Spacer()
                        Button("Open Login Items") { LaunchAtLogin.openLoginItemsSettings() }
                    }
                }
            } footer: {
                Text("Edgeway runs quietly in the menu bar and reconnects to your devices on its own.")
                    .settingsFootnote()
            }

            if model.role == .primary {
                Section {
                    Picker("Edge resistance", selection: Binding(
                        get: { Self.closestResistance(model.snapshot.edgeResistance) },
                        set: { model.setEdgeResistance($0) }
                    )) {
                        ForEach(Self.resistances, id: \.1) { Text($0.0).tag($0.1) }
                    }
                    LabeledContent("Bring control back") { ShortcutText("⌃⌥⌘H") }
                    LabeledContent("Pause or resume sharing") { ShortcutText("⌃⌥⌘P") }
                } header: {
                    Text("Switching")
                } footer: {
                    Text("Push the pointer past a screen edge to move to the computer next to it. Resistance keeps it from slipping across when you only meant to reach the menu bar or the Dock.")
                        .settingsFootnote()
                }
            }

            Section("Permissions") {
                PermissionRow(
                    title: "Accessibility",
                    detail: model.role == .secondary
                        ? "Lets Edgeway type and move the pointer for the Mac that controls this one."
                        : "Lets Edgeway send your keyboard and trackpad to another computer.",
                    granted: model.accessibilityGranted,
                    action: {
                        model.requestAccessibility()
                        Permissions.openAccessibilitySettings()
                    }
                )
            }
        }
        .formStyle(.grouped)
        .frame(width: 560)
        .fixedSize(horizontal: false, vertical: true)
        .onAppear { model.recheckPermissions() }
    }

    private static func closestResistance(_ value: Double) -> Double {
        resistances.map(\.1).min(by: { abs($0 - value) < abs($1 - value) }) ?? 40
    }
}

struct PermissionRow: View {
    var title: String
    var detail: String
    var granted: Bool
    var action: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Image(systemName: granted ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                .font(.title2)
                .foregroundStyle(granted ? Color.green : Color.orange)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(detail).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            if granted {
                Text("Allowed").foregroundStyle(.secondary)
            } else {
                Button("Allow…", action: action)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

struct ShortcutText: View {
    var keys: String
    init(_ keys: String) { self.keys = keys }

    var body: some View {
        Text(keys)
            .font(.system(.body, design: .rounded).weight(.medium))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(RoundedRectangle(cornerRadius: 5).fill(Color(nsColor: .quaternaryLabelColor)))
    }
}

extension Text {
    func settingsFootnote() -> some View {
        self.font(.callout).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Devices

struct DevicesSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var deviceToForget: DeviceRecord?

    var body: some View {
        Form {
            Section {
                if model.snapshot.devices.isEmpty {
                    Text(model.role == .primary ? "No paired devices yet. Choose a computer below to pair with it." : "No Mac has paired with this one yet.")
                        .foregroundStyle(.secondary)
                }
                ForEach(model.snapshot.devices) { device in
                    HStack(spacing: 12) {
                        Image(systemName: device.symbol)
                            .font(.title2)
                            .frame(width: 32)
                            .foregroundStyle(.secondary)
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(device.name)
                            HStack(spacing: 6) {
                                StatusDot(device: device)
                                Text(device.statusText).font(.callout).foregroundStyle(.secondary)
                            }
                            if let error = device.error {
                                Text(error).font(.caption).foregroundStyle(.orange)
                            }
                        }
                        Spacer()
                        Text(device.shortId).font(.caption.monospaced()).foregroundStyle(.tertiary)
                        Button("Forget…") { deviceToForget = device }
                    }
                    .padding(.vertical, 2)
                }
            } header: {
                Text(model.role == .primary ? "My Devices" : "Trusted Devices")
            }

            if model.role == .primary {
                Section {
                    if model.snapshot.nearby.isEmpty {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text("Looking for computers running Edgeway…").foregroundStyle(.secondary)
                        }
                    }
                    ForEach(model.snapshot.nearby) { nearby in
                        HStack(spacing: 12) {
                            Image(systemName: nearby.platform == "macos" ? "desktopcomputer" : "pc")
                                .font(.title2)
                                .frame(width: 32)
                                .foregroundStyle(.secondary)
                                .accessibilityHidden(true)
                            Text(nearby.instance)
                            Spacer()
                            Button("Pair…") { model.pair(with: nearby.instance) }
                                .disabled(model.pairing != nil)
                        }
                    }
                } header: {
                    Text("Nearby")
                } footer: {
                    Text("Open Edgeway on the other computer and choose “Is controlled by another computer”. It appears here when both are on the same network.")
                        .settingsFootnote()
                }
            } else {
                Section {
                    Text("To pair, open Edgeway on the Mac whose keyboard you want to use and choose “\(model.snapshot.name)” under Nearby. A six-digit code will appear here; type it on that Mac.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 560)
        .frame(minHeight: 300)
        .alert(item: $deviceToForget) { device in
            Alert(
                title: Text("Forget “\(device.name)”?"),
                message: Text("It will disconnect, and you’ll need to pair again with a new code to use it."),
                primaryButton: .destructive(Text("Forget Device")) { model.forget(device) },
                secondaryButton: .cancel()
            )
        }
    }
}

// MARK: - Arrangement

struct ArrangementSettingsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Drag your devices to match where they sit on your desk. Move the pointer past the edge where two devices meet to switch between them.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if model.role == .primary {
                ArrangementView()
                    .frame(height: 320)
            } else {
                ContentUnavailableLabel(
                    symbol: "rectangle.on.rectangle",
                    title: "Arranged on the controlling Mac",
                    message: "The arrangement is set on the Mac whose keyboard and trackpad you use."
                )
                .frame(height: 320)
            }
        }
        .padding(20)
        .frame(width: 620)
    }
}

/// A calm placeholder in the style of macOS empty states.
struct ContentUnavailableLabel: View {
    var symbol: String
    var title: String
    var message: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: symbol).font(.system(size: 34)).foregroundStyle(.tertiary)
            Text(title).font(.headline)
            Text(message).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: .controlBackgroundColor)))
    }
}
