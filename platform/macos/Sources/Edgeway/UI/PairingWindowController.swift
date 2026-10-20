import AppKit
import Combine
import SwiftUI

/// Shows the right pairing window for the current pairing state: a large code
/// on the Mac being paired, a code field on the Mac doing the pairing.
final class PairingWindowController {
    private let model: AppModel
    private var panel: NSPanel?
    private var cancellable: AnyCancellable?
    private var autoClose: DispatchWorkItem?

    init(model: AppModel) {
        self.model = model
        cancellable = model.$pairing
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] state in self?.update(state) }
    }

    private func update(_ state: PairingState?) {
        autoClose?.cancel()
        guard let state else {
            panel?.orderOut(nil)
            panel = nil
            return
        }
        let panel = self.panel ?? makePanel()
        self.panel = panel
        if case .showingCode = state {
            // The controlled Mac may have nobody at its keyboard: stay on top
            // of everything, on every Space, without stealing focus.
            panel.level = .statusBar
            panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
            panel.orderFrontRegardless()
        } else {
            panel.level = .floating
            panel.collectionBehavior = [.moveToActiveSpace]
            AppActivation.bringToFront()
            panel.makeKeyAndOrderFront(nil)
        }
        panel.center()

        switch state {
        case .succeeded:
            schedule(after: 1.6) { [weak self] in self?.model.dismissPairing() }
        case .failed:
            if model.role == .secondary { schedule(after: 5) { [weak self] in self?.model.dismissPairing() } }
        default:
            break
        }
    }

    private func schedule(after seconds: Double, _ work: @escaping () -> Void) {
        let item = DispatchWorkItem(block: work)
        autoClose = item
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: item)
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 420, height: 300),
            styleMask: [.titled, .fullSizeContentView],
            backing: .buffered,
            defer: true
        )
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.isMovableByWindowBackground = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        let host = NSHostingController(rootView: PairingView().environmentObject(model))
        host.sizingOptions = [.preferredContentSize]
        panel.contentViewController = host
        return panel
    }
}

struct PairingView: View {
    @EnvironmentObject private var model: AppModel
    @State private var code = ""
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(spacing: 16) {
            AppIconView(size: 64)
            content
        }
        .padding(.horizontal, 28)
        .padding(.top, 26)
        .padding(.bottom, 22)
        .frame(width: 420)
        .background(.regularMaterial)
    }

    @ViewBuilder
    private var content: some View {
        switch model.pairing {
        case .showingCode(let peer, let code):
            Text("Pair with \(peer)").font(.title2.weight(.semibold))
            Text("Enter this code on \(peer) to let it use this Mac.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            Text(Self.grouped(code))
                .font(.system(size: 52, weight: .semibold, design: .rounded).monospacedDigit())
                .tracking(4)
                .textSelection(.enabled)
                .padding(.vertical, 4)
                .accessibilityLabel("Code \(code.map(String.init).joined(separator: " "))")
            Button("Cancel") { model.cancelPairing() }
                .keyboardShortcut(.cancelAction)

        case .enteringCode(let peer):
            Text("Pair with \(peer)").font(.title2.weight(.semibold))
            Text("Enter the six-digit code shown on \(peer).")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            CodeField(code: $code, onComplete: submit)
                .focused($fieldFocused)
                .onAppear {
                    code = ""
                    fieldFocused = true
                }
            HStack {
                Button("Cancel") { model.cancelPairing() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Pair") { submit() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(code.count != 6)
            }

        case .working(let peer):
            Text("Connecting to \(peer)…").font(.title3.weight(.semibold))
            ProgressView().controlSize(.regular)
            Button("Cancel") { model.cancelPairing() }
                .keyboardShortcut(.cancelAction)

        case .succeeded(let peer):
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 34))
                .foregroundStyle(.green)
                .accessibilityHidden(true)
            Text(model.role == .primary ? "\(peer) is ready" : "Paired with \(peer)")
                .font(.title3.weight(.semibold))
            Text(model.role == .primary
                 ? "Move the pointer past the top of your screen to use it. You can change this in Arrange Displays."
                 : "\(peer) can now use this Mac. It will reconnect on its own.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)

        case .failed(let peer, let reason):
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 30))
                .foregroundStyle(.orange)
                .accessibilityHidden(true)
            Text("Couldn’t pair with \(peer)").font(.title3.weight(.semibold))
            Text(reason).multilineTextAlignment(.center).foregroundStyle(.secondary)
            HStack {
                Button("Close") { model.dismissPairing() }
                    .keyboardShortcut(.cancelAction)
                if model.role == .primary {
                    Spacer()
                    Button("Try Again") { model.retryPairing() }
                        .keyboardShortcut(.defaultAction)
                }
            }

        case nil:
            EmptyView()
        }
    }

    private func submit() {
        guard code.count == 6 else { return }
        model.submitCode(code)
    }

    static func grouped(_ code: String) -> String {
        guard code.count == 6 else { return code }
        return "\(code.prefix(3)) \(code.suffix(3))"
    }
}

/// Six-digit entry: one field, large rounded digits, digits only, submits on
/// the sixth digit.
struct CodeField: View {
    @Binding var code: String
    var onComplete: () -> Void

    var body: some View {
        TextField("000 000", text: Binding(
            get: { PairingView.grouped(code) },
            set: { newValue in
                let digits = String(newValue.filter(\.isNumber).prefix(6))
                if digits != code {
                    code = digits
                    if digits.count == 6 { DispatchQueue.main.async(execute: onComplete) }
                }
            }
        ))
        .textFieldStyle(.plain)
        .font(.system(size: 40, weight: .semibold, design: .rounded).monospacedDigit())
        .multilineTextAlignment(.center)
        .padding(.vertical, 8)
        .padding(.horizontal, 16)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .textBackgroundColor))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Color(nsColor: .separatorColor))
        )
        .frame(width: 240)
        .accessibilityLabel("Pairing code")
    }
}
