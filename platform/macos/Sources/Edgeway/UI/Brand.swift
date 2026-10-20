import AppKit
import SwiftUI

/// Edgeway's visual identity: the app icon and the menu bar glyph derived
/// from it — a pointer crossing a vertical edge, solid on one side and open
/// on the other.
enum Brand {
    static let tagline = "One keyboard. One trackpad. Every computer."
    static let author = "Rafe Uddaraj"

    /// The full-colour app icon from the bundle.
    static var appIcon: NSImage {
        NSApp.applicationIconImage ?? NSImage(named: NSImage.applicationIconName) ?? NSImage()
    }

    /// Template glyph for the menu bar. `dimmed` when nothing is connected.
    static func menuBarGlyph(active: Bool, paused: Bool) -> NSImage {
        let size = NSSize(width: 18, height: 18)
        let image = NSImage(size: size, flipped: true) { _ in
            let alpha: CGFloat = paused ? 0.45 : 1
            NSColor.black.withAlphaComponent(alpha).set()

            // The edge.
            let edge = NSBezierPath()
            edge.move(to: NSPoint(x: 8.5, y: 1))
            edge.line(to: NSPoint(x: 8.5, y: 17))
            edge.lineWidth = 1.1
            edge.lineCapStyle = .round
            edge.stroke()

            // The pointer, tip at the top-left.
            let arrow = NSBezierPath()
            arrow.move(to: NSPoint(x: 5.2, y: 3.2))
            arrow.line(to: NSPoint(x: 13.6, y: 9.4))
            arrow.line(to: NSPoint(x: 9.9, y: 9.9))
            arrow.line(to: NSPoint(x: 12.0, y: 14.3))
            arrow.line(to: NSPoint(x: 10.3, y: 15.1))
            arrow.line(to: NSPoint(x: 8.3, y: 10.8))
            arrow.line(to: NSPoint(x: 5.6, y: 13.4))
            arrow.close()
            arrow.lineJoinStyle = .round

            // Solid where it has been, outlined where it is going.
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(rect: NSRect(x: 0, y: 0, width: 8.5, height: 18)).addClip()
            arrow.fill()
            NSGraphicsContext.restoreGraphicsState()
            arrow.lineWidth = active ? 1.2 : 1.0
            arrow.stroke()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Edgeway"
        return image
    }

    /// Small status dot used in menus.
    static func statusDot(_ color: NSColor) -> NSImage {
        let image = NSImage(size: NSSize(width: 8, height: 8), flipped: false) { rect in
            color.setFill()
            NSBezierPath(ovalIn: rect.insetBy(dx: 0.5, dy: 0.5)).fill()
            return true
        }
        return image
    }
}

/// The app icon as a SwiftUI view.
struct AppIconView: View {
    var size: CGFloat = 64

    var body: some View {
        Image(nsImage: Brand.appIcon)
            .resizable()
            .interpolation(.high)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// Green / orange / grey status indicator.
struct StatusDot: View {
    var device: DeviceRecord

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 8, height: 8)
            .accessibilityLabel(device.statusText)
    }

    private var color: Color {
        if device.isConnected { return .green }
        if device.isConnecting { return .orange }
        return Color(nsColor: .tertiaryLabelColor)
    }
}
