# What Edgeway can and can't do

Edgeway is an ordinary app with ordinary permissions. It doesn't pretend to
get around operating-system security, and this page says exactly where those
boundaries are, why, and what you can do about each.

## Before anyone logs in

| Screen | Can Edgeway control it? | Why |
|---|---|---|
| **FileVault / encrypted-disk password at boot** | **No. No software can.** | The Mac starts a minimal pre-boot environment that loads no third-party code until the disk is unlocked. Windows BitLocker with a PIN and Linux LUKS work the same way. |
| **macOS login window** (FileVault off, automatic login off) | No, not today | Edgeway runs inside a user session, started by Login Items. Injecting input at the login window would need a privileged helper loaded into the login-window session. That is not something we will ship without verifying it on current macOS. It is tracked as research, not promised. |
| **Lock screen / screen saver password** | Not verified yet | Edgeway keeps running while the screen is locked, but whether macOS accepts posted input in the unlock field needs testing on real hardware. Until then, avoid locking the controlled Mac (see below). |
| **After login** | Yes | Everything below applies. |

### What to do instead on a Mac mini

- **Simplest:** turn FileVault off and turn on automatic login. The Mac mini
  starts, logs in, starts Edgeway, and your MacBook reconnects on its own.
  The trade-off: someone with physical access to the Mac mini can read its
  disk.
- **Keep FileVault:** macOS 26 Tahoe on Apple silicon can unlock FileVault
  over SSH. Turn on **Remote Login** (System Settings › General › Sharing)
  *before* you need it; after a restart run `ssh you@mac-mini.local` from the
  MacBook and enter the password. Reports say this works over Wi-Fi from
  macOS 26.5. We haven't yet verified whether the Mac then continues to the
  desktop or stops at the login window. Please report what you see.
- **Avoid lock-screen trouble:** on the Mac mini, set System Settings › Lock
  Screen › "Require password after screen saver begins or display is turned
  off" to a longer interval or Never if it is in a trusted place.
- **Future option:** a tiny USB device (for example an ESP32-S3) plugged
  into the Mac mini that presents itself as a real keyboard and mouse would
  work even at the FileVault screen, because to the Mac it *is* hardware. It
  is a possible future add-on, not part of this release.

### "Start Edgeway when you log in" vs. "before login"

Edgeway uses `SMAppService`, Apple's supported Login Items mechanism. It
starts Edgeway **after** a user logs in, every time, with no Terminal setup.
"Before login" would need a system-wide launch agent bound to the login
window, which has different permissions and is not supported for this kind
of app. That is why the restart behaviour above depends on automatic login.

## Secure Input

When a password field is focused, or Terminal's *Secure Keyboard Entry* is on,
macOS turns on **Secure Event Input**, and *no* app can read keystrokes
system-wide. If that happens on the sharing Mac while another computer has
focus, your typing would silently go to the password field. Edgeway checks for
this four times a second while away and **brings control home** with a short
notice instead. Use ⌃⌥⌘H at any time to return manually; it works even while
Secure Input is on.

On the *controlled* Mac, Edgeway can type into password fields normally.

Some apps forget to turn Secure Input off (it then stays on system-wide until
they quit). If control keeps bouncing back, find the culprit with:

```bash
ioreg -l -w 0 | grep kCGSSessionSecureInputPID
```

## Trackpad gestures

| Gesture | Works on the controlled Mac? |
|---|---|
| Pointer movement, click, right-click, drag | Yes |
| Two-finger scrolling, with momentum and rubber-banding | Yes |
| Tap to click, force click | Yes (as clicks) |
| Pinch to zoom, rotate, smart zoom | No. macOS has no public API to create these. |
| Three/four-finger swipes (Spaces, Mission Control) | No, for the same reason. Use the keyboard shortcuts (⌃← / ⌃→, ⌃↑) instead. |

Gestures are still blocked on the sharing Mac while another computer has
focus, so they can't trigger anything locally by accident.

## Other things to know

- **Same network.** Both computers must be on the same local network and able
  to see each other. Guest Wi-Fi and networks with client isolation block
  this. VPNs that capture all traffic can too.
- **Wi-Fi latency.** Edgeway marks its packets for Wi-Fi's low-latency queue
  and turns off Nagle's algorithm. On a busy or distant access point you may
  still see occasional delays; Ethernet on the Mac mini helps the most.
- **Clipboard and file drag-and-drop** are not shared. Edgeway sends only
  input.
- **Media keys** (volume, brightness, play/pause) are not forwarded yet.
  While another computer has focus they do nothing, so they can't surprise
  you on the wrong machine.
- **Caps Lock** is mirrored to the controlled Mac as a lock state.
- **Hiding the cursor** while another computer has focus uses a
  long-standing but undocumented WindowServer setting, because macOS offers no
  public API for a menu bar app to hide the cursor. If a future macOS removes
  it, the cursor stays visible, parked at the screen edge, and everything else
  keeps working.
- **Crossing with a button held.** Edgeway doesn't switch computers while a
  mouse button is held, so drags never end up split across two machines.

## Windows and Linux

Not released yet. Expected boundaries: on Windows, controlling administrator
windows, UAC prompts and the sign-in screen requires the optional background
service; on Linux Wayland, the desktop asks once for permission and some
compositors need the `uinput` rule in [INSTALL.md](INSTALL.md#linux-planned).
