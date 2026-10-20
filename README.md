<p align="center">
  <img src="assets/brand/edgeway-icon.png" width="128" height="128" alt="Edgeway icon">
</p>

<h1 align="center">Edgeway</h1>

<p align="center"><strong>One keyboard. One trackpad. Every computer.</strong></p>

<p align="center">
  Move your pointer past the edge of your screen and keep working on the computer next to it.<br>
  No accounts. No cloud. No extra keyboard and mouse. About 1.3 MB.
</p>

<p align="center">
  <a href="docs/INSTALL.md">Install</a> ·
  <a href="docs/BUILDING.md">Build from source</a> ·
  <a href="docs/LIMITATIONS.md">Limitations</a> ·
  <a href="docs/SECURITY.md">Security</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a>
</p>

---

Edgeway lets the keyboard and trackpad of one computer control the others on
your desk. Each computer keeps its own display. Only input travels between
them, encrypted, directly over your local network.

```
            ┌──────────────────────┐
            │       Mac mini       │   ← push the pointer through the top edge
            │  (its own monitor)   │
            └──────────┬───────────┘
                ┌──────┴──────┐
                │ MacBook Pro │       ← the keyboard and trackpad you use
                └─────────────┘
```

## Why another one?

Apple's Universal Control is lovely when it works, but it needs the same
Apple ID and iCloud on every Mac, works only between Apple devices and only
after someone logs in, and often doesn't come back after a restart or sleep.
The open-source alternatives work across platforms but are large, configured
with IP addresses and config files, and don't feel at home on a Mac.

Edgeway aims to be the small, native, reliable one:

|                              | Edgeway | Universal Control | Deskflow / Synergy | Lan Mouse |
|------------------------------|:-------:|:-----------------:|:------------------:|:---------:|
| No account or cloud          | ✓ | Apple ID + iCloud | ✓ | ✓ |
| Pair with a six-digit code   | ✓ | — | manual config | manual config |
| Reconnects on its own        | ✓ | not always | ✓ | ✓ |
| Encrypted and authenticated  | ✓ always | ✓ | TLS | DTLS |
| Native Mac interface         | ✓ | ✓ | Qt | GTK |
| Mac, Windows, Linux          | Mac now; Windows & Linux planned | Apple only | ✓ | ✓ |
| Free and open source         | ✓ MIT | — | GPL / paid | GPL |

## Features

- **Edge switching.** Push the pointer past a screen edge to move to the
  computer arranged there. By default your second computer sits *above* this
  one. Rearrange everything by dragging, as in System Settings › Displays.
- **No collisions.** While another computer has focus, this Mac's cursor is
  frozen and hidden and none of your typing, clicks, scrolling or gestures
  reach it. Back home, the other computer receives nothing.
- **No stuck keys.** A key is always released on the computer that saw it
  pressed. If the connection drops, the other computer releases everything on
  its own within a few seconds.
- **Secure by default.** Noise protocol encryption with forward secrecy;
  devices pair once with a six-digit code verified by SPAKE2; an unpaired
  device can never inject input.
- **Quietly reliable.** Lives in the menu bar, starts when you log in,
  rediscovers devices with Bonjour and reconnects after sleep, network changes
  and restarts. Idle CPU use is effectively zero.
- **Smooth.** Trackpad scrolling keeps its phases and momentum; Nagle is off
  and packets are marked for Wi-Fi's low-latency queue.
- **Safety valves.** ⌃⌥⌘H brings control home from anywhere; ⌃⌥⌘P pauses
  sharing. When a password field turns on Secure Input, Edgeway brings control
  home instead of letting keystrokes land in the wrong place.

## Measured, not claimed

On an M1 MacBook Pro running macOS 26.5 (see [docs/TESTING.md](docs/TESTING.md)):

| | |
|---|---|
| App size (Apple silicon) | **1.32 MB** · download **0.55 MB** |
| App size (universal) | 2.72 MB · download 1.12 MB |
| Memory footprint, idle | **11 MB** |
| CPU, idle for 60 s | **0.02 s** total (≈0.0 %) |
| Pairing, end to end | 110 ms |
| Reconnect after restart | 50 ms once the device is visible |
| Round trip, loopback | 0.28 ms |

## Quick start

1. Install Edgeway on both computers ([details](docs/INSTALL.md)):
   ```bash
   curl -fsSL https://raw.githubusercontent.com/rafeuddaraj/edgeway/main/scripts/install.sh | bash
   ```
2. On the MacBook choose **Share my keyboard and trackpad**; on the Mac mini
   choose **Be controlled by another computer**. Allow Accessibility on both.
3. On the MacBook open the Edgeway menu, pick the Mac mini under **Nearby**
   and type the six-digit code shown on the Mac mini's screen.
4. Push the pointer through the top of the MacBook's screen.

Setting up a Mac mini that has no keyboard of its own, and making it come
back after a restart without anyone touching it, is covered in
[docs/INSTALL.md](docs/INSTALL.md#a-mac-mini-without-its-own-keyboard).

## Honest limitations

Edgeway is a regular app. It starts after a user logs in, so it cannot type
your FileVault password at boot or work at the login window, and no software
can. macOS 26 can unlock FileVault over SSH, and a Mac mini with automatic
login needs no help at all. Trackpad pinch and rotate gestures can't be
recreated through public macOS APIs. Everything is explained in
[docs/LIMITATIONS.md](docs/LIMITATIONS.md).

## Platforms

| Platform | Status |
|---|---|
| macOS 13 Ventura or later (Apple silicon and Intel) | ✅ Available |
| Windows 10/11 | 🛠 Planned: same core, native Win32 shell ([plan](platform/windows/README.md)) |
| Linux (X11, GNOME/KDE Wayland) | 🛠 Planned: same core, portal-based shell ([plan](platform/linux/README.md)) |

The networking, security, protocol and switching logic live in a portable
Rust core ([core/](core)), so each new platform needs only input capture,
input injection and a small native interface.

## Project layout

```
core/               Rust engine shared by every platform (tests in core/tests)
platform/macos/     Swift + AppKit menu bar app
platform/windows/   Plan for the Windows shell
platform/linux/     Plan for the Linux shell
scripts/            Build, install, signing, icon and preview tools
docs/               Installation, building, design and security documents
assets/brand/       Logo and icon
```

## Contributing

Bug reports, testing on more hardware and the Windows and Linux shells are
all welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2026 Rafe Uddaraj
