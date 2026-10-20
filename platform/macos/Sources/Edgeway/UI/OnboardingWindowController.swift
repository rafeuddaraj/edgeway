import AppKit
import SwiftUI

/// First-run setup: choose what this Mac does, grant Accessibility, and decide
/// whether Edgeway starts at login.
final class OnboardingWindowController: NSWindowController, NSWindowDelegate {
    var onClose: (() -> Void)?

    init(model: AppModel) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 520),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        super.init(window: window)
        window.delegate = self
        let host = NSHostingController(rootView: OnboardingView(close: { [weak window] in window?.close() }).environmentObject(model))
        host.sizingOptions = [.preferredContentSize]
        window.contentViewController = host
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show() {
        window?.center()
        AppActivation.bringToFront()
        window?.makeKeyAndOrderFront(nil)
        window?.orderFrontRegardless()
    }

    func windowWillClose(_ notification: Notification) {
        onClose?()
    }
}

struct OnboardingView: View {
    @EnvironmentObject private var model: AppModel
    var close: () -> Void
    @State private var step = 0
    @State private var chosen: Role = .primary
    @State private var startAtLogin = true

    var body: some View {
        VStack(spacing: 0) {
            Group {
                switch step {
                case 0: welcome
                case 1: permissions
                default: finish
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .padding(.horizontal, 36)
            .padding(.top, 40)

            Divider()
            HStack {
                if step > 0 {
                    Button("Back") { step -= 1 }
                }
                Spacer()
                Button(step == 2 ? "Done" : "Continue", action: next)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(16)
        }
        .frame(width: 560, height: 520)
        .background(.background)
        .onAppear { chosen = model.role ?? .primary }
    }

    private func next() {
        switch step {
        case 0:
            model.setRole(chosen)
            step = 1
        case 1:
            step = 2
        default:
            model.setLaunchAtLogin(startAtLogin)
            model.finishOnboarding()
            close()
        }
    }

    private var welcome: some View {
        VStack(spacing: 14) {
            AppIconView(size: 96)
            Text("Welcome to Edgeway").font(.largeTitle.weight(.semibold))
            Text(Brand.tagline).foregroundStyle(.secondary)
            Text("What should this Mac do?")
                .font(.headline)
                .padding(.top, 14)
            HStack(spacing: 14) {
                RoleCard(
                    symbol: "laptopcomputer",
                    title: "Share my keyboard and trackpad",
                    detail: "Use this Mac to control your other computers.",
                    selected: chosen == .primary
                ) { chosen = .primary }
                RoleCard(
                    symbol: "macmini",
                    title: "Be controlled by another computer",
                    detail: "Use the keyboard and trackpad of another Mac here.",
                    selected: chosen == .secondary
                ) { chosen = .secondary }
            }
        }
    }

    private var permissions: some View {
        VStack(spacing: 16) {
            Image(systemName: "hand.raised.fill")
                .font(.system(size: 44))
                .foregroundStyle(Color.accentColor)
                .accessibilityHidden(true)
            Text("Allow Accessibility").font(.title.weight(.semibold))
            Text(chosen == .primary
                 ? "Edgeway needs Accessibility access to send your keyboard and trackpad to another computer and keep them from reaching this Mac while you work there."
                 : "Edgeway needs Accessibility access to type and move the pointer on this Mac for the computer that controls it.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            PermissionRow(
                title: "Accessibility",
                detail: "System Settings › Privacy & Security › Accessibility",
                granted: model.accessibilityGranted,
                action: {
                    model.requestAccessibility()
                    Permissions.openAccessibilitySettings()
                }
            )
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color(nsColor: .controlBackgroundColor)))
            Text("Edgeway works only on your local network. It has no accounts, no cloud and no analytics. Keystrokes travel encrypted, directly between your computers.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
        }
        .onAppear { model.recheckPermissions() }
    }

    private var finish: some View {
        VStack(spacing: 16) {
            Image(systemName: "checkmark.seal.fill")
                .font(.system(size: 44))
                .foregroundStyle(.green)
                .accessibilityHidden(true)
            Text("You’re all set").font(.title.weight(.semibold))
            Text(chosen == .primary
                 ? "Open Edgeway on your other computer too. It will appear in the Edgeway menu under Nearby — choose it and type the code it shows."
                 : "Now open Edgeway on the Mac whose keyboard you want to use, choose this Mac under Nearby, and type the code that appears here.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Toggle("Start Edgeway when you log in", isOn: $startAtLogin)
                .toggleStyle(.switch)
                .padding(14)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color(nsColor: .controlBackgroundColor)))
            Text("Edgeway lives in the menu bar. Look for the pointer crossing an edge.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }
}

private struct RoleCard: View {
    var symbol: String
    var title: String
    var detail: String
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 10) {
                Image(systemName: symbol)
                    .font(.system(size: 34))
                    .foregroundStyle(selected ? Color.accentColor : .secondary)
                Text(title).font(.headline).multilineTextAlignment(.center)
                Text(detail).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.center)
            }
            .padding(16)
            .frame(maxWidth: .infinity, minHeight: 170)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(selected ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: selected ? 2 : 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
