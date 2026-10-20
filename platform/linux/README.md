# Edgeway for Linux (planned)

The Linux shell reuses the Rust core unchanged. It is not implemented yet.
This is the plan.

| Piece | X11 | Wayland |
|---|---|---|
| Capture | XInput2 raw events with an active grab while away | `org.freedesktop.portal.InputCapture` with libei (GNOME 45+, KDE Plasma 6.1+) |
| Cursor | Pointer barriers / grab and a blank cursor | Handled by the InputCapture portal |
| Injection | XTest | `org.freedesktop.portal.RemoteDesktop` with libei; `uinput` fallback for wlroots compositors |
| Discovery | Avahi (D-Bus or `libdns_sd` compatibility), as a backend in `core/src/discovery.rs` | same |
| UI | GTK 4 or a StatusNotifierItem tray icon with a small settings window, following GNOME/KDE conventions | same |
| Autostart | `~/.config/autostart/edgeway.desktop` | same |
| Distribution | AppImage, `.deb`, `.rpm`, Flathub | same |

Wayland portals ask the user for permission once and return a restore token
so the prompt does not repeat. Login screens (GDM, SDDM) are out of reach
for a user application on Wayland.

Install-time notes for users are already in
[docs/INSTALL.md](../../docs/INSTALL.md#linux-planned).
