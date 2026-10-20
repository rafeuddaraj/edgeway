# Edgeway for Windows (planned)

The Windows shell reuses the Rust core unchanged and adds the Windows-specific
pieces. It is not implemented yet. This is the plan, so contributors can pick
it up.

| Piece | Approach |
|---|---|
| Language / UI | Rust with the `windows` crate; a notification-area (tray) icon with a native menu; settings and arrangement in a small Win32 window. Target: under 2 MB. |
| Capture | `SetWindowsHookEx` low-level keyboard and mouse hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`) on a dedicated thread; return non-zero to swallow while another device has focus. Raw Input for precise relative movement. |
| Cursor | `ClipCursor` to a 1×1 rectangle plus a transparent cursor while away; `SetCursorPos` on return. |
| Injection | `SendInput` with scan codes (HID usage → Windows scan code table in `core/src/keymap.rs`). |
| Discovery | Windows 10 1809+ `DnsServiceRegister` / `DnsServiceBrowse` (`windns.h`), added as a backend in `core/src/discovery.rs`. |
| Start at sign-in | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. |
| Admin windows, UAC and the sign-in screen | Optional Windows service running as SYSTEM that launches a helper on the Winlogon desktop. Without it, Windows blocks input into elevated windows (UIPI). |
| Ctrl+Alt+Del | `SendSAS`, which requires the service and a Group Policy setting. |
| Distribution | Signed installer (Azure Trusted Signing or an OV certificate) to avoid SmartScreen warnings; `winget` package. |

Modifier mapping between Mac and Windows (⌘ ↔ Ctrl) will be a setting on the
controlling device, applied in the core.

Install-time notes for users are already in
[docs/INSTALL.md](../../docs/INSTALL.md#windows-planned).
