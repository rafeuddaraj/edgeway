// Renders Edgeway's windows with sample data to PNG files, in light and dark
// appearance, for design review and the README. Not part of the app.
//
//   scripts/render-previews.sh [output-directory]
//
// Copyright (c) 2026 Rafe Uddaraj. MIT License.

import AppKit
import SwiftUI

let output = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "build/previews")
try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
if let iconPath = CommandLine.arguments.dropFirst(2).first, let icon = NSImage(contentsOfFile: iconPath) {
    app.applicationIconImage = icon
}

func screen(_ x: Double, _ y: Double, _ w: Double, _ h: Double, _ name: String) -> ScreenRecord {
    ScreenRecord(x: x, y: y, w: w, h: h, name: name)
}

let mini = DeviceRecord(
    id: String(repeating: "a", count: 64), shortId: "3F2A-91C0", name: "Mac mini", platform: "macos",
    status: "connected", online: true, rttMs: 3.2, error: nil, placement: Placement(x: -524, y: -1440),
    screens: [screen(0, 0, 2560, 1440, "Studio Display")]
)
let pc = DeviceRecord(
    id: String(repeating: "b", count: 64), shortId: "77C1-0B9E", name: "Office PC", platform: "windows",
    status: "offline", online: false, rttMs: nil, error: nil, placement: Placement(x: 1512, y: 0),
    screens: [screen(0, 0, 1920, 1080, "DELL U2723QE")]
)
let primarySnapshot = Snapshot(
    role: "primary", name: "Rafe’s MacBook Pro", id: String(repeating: "c", count: 64), shortId: "61D5-2135",
    paused: false, focus: nil, controlledBy: nil, edgeResistance: 40, port: 0,
    screens: [screen(0, 0, 1512, 982, "Built-in Liquid Retina XDR Display")],
    devices: [mini, pc],
    nearby: [NearbyRecord(instance: "Mac Studio", fingerprint: "0011223344556677", platform: "macos")],
    pairing: nil
)
var secondarySnapshot = primarySnapshot
secondarySnapshot.role = "secondary"
secondarySnapshot.name = "Mac mini"
secondarySnapshot.devices = [DeviceRecord(
    id: String(repeating: "d", count: 64), shortId: "61D5-2135", name: "Rafe’s MacBook Pro", platform: "macos",
    status: "connected", online: true, rttMs: 2.8, error: nil, placement: nil, screens: []
)]
secondarySnapshot.nearby = []

func render<V: View>(_ name: String, model: AppModel, width: CGFloat? = nil, _ view: () -> V) {
    for (suffix, appearance) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
        let host = NSHostingView(rootView: view().environmentObject(model))
        host.appearance = NSAppearance(named: appearance)
        var size = host.fittingSize
        if let width { size.width = width }
        size.height = max(size.height, 120)
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: appearance)
        window.backgroundColor = .windowBackgroundColor
        window.contentView = host
        host.frame = NSRect(origin: .zero, size: size)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.15))
        guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { continue }
        host.cacheDisplay(in: host.bounds, to: rep)
        let url = output.appendingPathComponent("\(name)-\(suffix).png")
        try? rep.representation(using: .png, properties: [:])?.write(to: url)
        print(url.path)
    }
}

let options = LaunchOptions(profile: "previews", role: .primary, dryRun: true)
let primary = AppModel(options: options)
primary.loadPreview(snapshot: primarySnapshot, role: .primary, pairing: nil, accessibilityGranted: true)
render("settings-general", model: primary) { GeneralSettingsView() }
render("settings-devices", model: primary) { DevicesSettingsView() }
render("settings-arrangement", model: primary) { ArrangementSettingsView() }
render("onboarding", model: primary) { OnboardingView(close: {}) }

let entering = AppModel(options: options)
entering.loadPreview(snapshot: primarySnapshot, role: .primary, pairing: .enteringCode(peer: "Mac mini"), accessibilityGranted: true)
render("pairing-enter-code", model: entering) { PairingView() }
let done = AppModel(options: options)
done.loadPreview(snapshot: primarySnapshot, role: .primary, pairing: .succeeded(peer: "Mac mini"), accessibilityGranted: true)
render("pairing-succeeded", model: done) { PairingView() }

let secondary = AppModel(options: options)
secondary.loadPreview(snapshot: secondarySnapshot, role: .secondary, pairing: .showingCode(peer: "Rafe’s MacBook Pro", code: "482913"), accessibilityGranted: false)
render("pairing-show-code", model: secondary) { PairingView() }
render("settings-general-secondary", model: secondary) { GeneralSettingsView() }
