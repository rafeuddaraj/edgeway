import AppKit
import CoreGraphics

/// Freezes and hides this Mac's cursor while another computer has focus, and
/// brings it back exactly where control returns.
///
/// macOS only lets the frontmost app hide the cursor. A menu bar app is never
/// frontmost, so Edgeway sets the WindowServer connection property
/// `SetsCursorInBackground` — the long-standing approach used by
/// keyboard-sharing tools. It is looked up at runtime; if it ever disappears,
/// the cursor simply stays visible, parked at the screen edge.
final class CursorController {
    private(set) var isAway = false
    private var hidden = false
    private var backgroundHidingEnabled = false

    func freeze(at point: CGPoint) {
        isAway = true
        CGAssociateMouseAndMouseCursorPosition(0)
        DispatchQueue.main.async { [weak self] in
            guard let self, self.isAway else { return }
            self.hide()
        }
    }

    func unfreeze(at point: CGPoint) {
        isAway = false
        CGWarpMouseCursorPosition(point)
        // Re-associating right after a warp avoids the quarter-second pause in
        // which macOS otherwise ignores pointer movement.
        CGAssociateMouseAndMouseCursorPosition(1)
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.isAway else { return }
            self.show()
        }
    }

    /// Always leave the cursor usable, e.g. when quitting.
    func restore() {
        isAway = false
        CGAssociateMouseAndMouseCursorPosition(1)
        show()
    }

    private func hide() {
        guard !hidden else { return }
        enableBackgroundHiding()
        CGDisplayHideCursor(CGMainDisplayID())
        hidden = true
    }

    private func show() {
        guard hidden else { return }
        CGDisplayShowCursor(CGMainDisplayID())
        hidden = false
    }

    private func enableBackgroundHiding() {
        guard !backgroundHidingEnabled else { return }
        typealias DefaultConnection = @convention(c) () -> Int32
        typealias SetProperty = @convention(c) (Int32, Int32, CFString, CFTypeRef) -> Int32
        let rtld = UnsafeMutableRawPointer(bitPattern: -2) // RTLD_DEFAULT
        guard let connSym = dlsym(rtld, "_CGSDefaultConnection"),
              let setSym = dlsym(rtld, "CGSSetConnectionProperty") else { return }
        let connection = unsafeBitCast(connSym, to: DefaultConnection.self)()
        let setProperty = unsafeBitCast(setSym, to: SetProperty.self)
        _ = setProperty(connection, connection, "SetsCursorInBackground" as CFString, kCFBooleanTrue)
        backgroundHidingEnabled = true
    }
}
