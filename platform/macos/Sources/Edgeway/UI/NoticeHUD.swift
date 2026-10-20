import AppKit
import SwiftUI

/// A brief, non-activating notice near the top of the screen, in the style of
/// the system volume and brightness HUDs. It never takes focus.
final class NoticeHUD {
    private var panel: NSPanel?
    private var hideWork: DispatchWorkItem?

    func show(title: String, message: String) {
        hideWork?.cancel()
        let panel = self.panel ?? makePanel()
        self.panel = panel
        panel.contentViewController = NSHostingController(rootView: NoticeView(title: title, message: message))
        if let screen = NSScreen.main {
            let size = panel.contentViewController?.view.fittingSize ?? NSSize(width: 340, height: 70)
            let frame = screen.visibleFrame
            panel.setFrame(
                NSRect(x: frame.midX - size.width / 2, y: frame.maxY - size.height - 24, width: size.width, height: size.height),
                display: true
            )
        }
        panel.alphaValue = 1
        panel.orderFrontRegardless()
        let work = DispatchWorkItem { [weak self] in self?.hide() }
        hideWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: work)
    }

    private func hide() {
        guard let panel else { return }
        NSAnimationContext.runAnimationGroup({ ctx in
            ctx.duration = 0.25
            panel.animator().alphaValue = 0
        }, completionHandler: { panel.orderOut(nil) })
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 340, height: 70),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: true
        )
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        return panel
    }
}

private struct NoticeView: View {
    var title: String
    var message: String

    var body: some View {
        HStack(spacing: 12) {
            AppIconView(size: 36)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline)
                Text(message).font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(width: 360, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}
