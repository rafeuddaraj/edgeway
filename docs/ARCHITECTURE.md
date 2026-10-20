# Architecture

Edgeway is split so that everything hard to get right lives in one place and
every platform gets a native app on top of it.

```
┌──────────────────────── platform shell (one per OS) ───────────────────────┐
│  UI: menu bar, settings, arrangement, pairing     (macOS: AppKit + SwiftUI)│
│  Input capture  ─────────┐            ┌──────────►  Input injection        │
│  (event tap)             │            │             (CGEventPost)          │
│  Cursor freeze / hide ◄──┼────┐       │                                    │
└──────────────────────────┼────┼───────┼────────────────────────────────────┘
                           ▼    │       │            C ABI (core/include/edgeway.h)
┌─────────────────────────────── core (Rust) ────────────────────────────────┐
│ switching ── keystate          injector                                    │
│     │                              ▲                                       │
│  engine: sessions, heartbeat, reconnection, pairing, snapshot              │
│     │                              │                                       │
│  protocol ── noise ── pairing ── identity ── config ── discovery (DNS-SD)  │
└────────────────────────────────────────────────────────────────────────────┘
```

## The core (`core/`)

| Module | Role |
|---|---|
| `identity` | Creates and loads the X25519 key pair; fingerprints |
| `noise` | Noise XX handshake; split sealer/opener so reader and writer threads never contend |
| `pairing` | Six-digit code, SPAKE2, key confirmation bound to the handshake, attempt throttling |
| `config` | Trusted devices, arrangement and settings as JSON, written atomically |
| `protocol` / `wire` | Versioned binary messages; unknown messages are ignored for forward compatibility |
| `keymap` | USB HID usages as the neutral key code; macOS ↔ HID table and modifier flags |
| `switching` | Arrangement geometry, edge detection with resistance, the virtual cursor, effects |
| `keystate` | Routes each key release to the device that saw the press |
| `injector` | Receiving side: turns messages into events and tracks what is held, to release it |
| `discovery` | DNS-SD advertise/browse (macOS `dns_sd`; Avahi / Bonjour for other shells) |
| `engine` | Threads, sessions, heartbeat, reconnection with backoff, snapshot, public API |
| `ffi` | The C ABI used by the shells |

The core has five dependencies: `snow` (Noise), `spake2`, `blake2`,
`nanoserde` (JSON) and `libc`, plus `getrandom`. There is no async runtime.

### Threads

| Thread | When | Blocks on |
|---|---|---|
| listener | controlled device | `accept()` |
| dialer | controlling device | condition variable (woken by discovery, disconnects, network changes) |
| heartbeat | always | condition variable; ticks once a second only while connected |
| reader + writer per connection | while connected | socket read / channel receive |
| discovery | always | `poll()` on the DNS-SD daemon socket |

Nothing polls. An idle Edgeway uses effectively no CPU.

### The switching model

All devices share one *arrangement space* in points, using the controlling
Mac's global display coordinates as the reference, just like System Settings ›
Displays. A remote device is its displays shifted by its placement.

- **At home**, the system moves the cursor. On each mouse event the switcher
  checks whether the movement leaves this Mac's screens. If a probe point
  just past that edge lands on another device's screen, it accumulates
  "push"; past the resistance threshold it crosses.
- **Away**, the switcher owns a virtual cursor in arrangement space, moves it
  by the raw deltas, clamps it to the remote screens, and sends **absolute**
  positions. The controlling Mac alone decides when the pointer comes back,
  so the two sides can never disagree.
- Crossing emits *effects*: freeze/hide the local cursor, send `Enter`, and
  later `Leave` and unfreeze at the matching point. Every local event while
  away is **swallowed**: the event tap returns `NULL`, so the sharing Mac
  receives nothing.
- No crossing happens while a mouse button is held.

### Never a stuck key

- `keystate` remembers who received each key press and routes the release to
  the same device, even after switching.
- The controlled device's `injector` remembers what it pressed. On
  `ReleaseAll`, disconnect, heartbeat timeout or pause it releases ordinary
  keys first, then modifiers, then mouse buttons.
- Forced returns (⌃⌥⌘H, Secure Input, pause, sleep) send `ReleaseAll`.

## The macOS shell (`platform/macos/`)

| Area | Files |
|---|---|
| App lifecycle, state | `App/AppDelegate.swift`, `App/AppModel.swift`, `App/LaunchOptions.swift` |
| Core bridge | `Core/EngineBridge.swift`, `Core/Snapshot.swift` |
| Input | `Input/EventTap.swift` (capture on a dedicated high-priority run loop), `Input/EventInjector.swift`, `Input/CursorController.swift`, `Input/Hotkeys.swift` (Carbon hot keys + Secure Input monitor) |
| System | `System/Displays.swift`, `System/Permissions.swift`, `System/LaunchAtLogin.swift` (`SMAppService`), `System/SystemEvents.swift` (sleep, wake, lock, display and network changes) |
| UI | `UI/StatusMenuController.swift`, `UI/SettingsWindowController.swift`, `UI/ArrangementView.swift`, `UI/PairingWindowController.swift`, `UI/OnboardingWindowController.swift`, `UI/NoticeHUD.swift`, `UI/Brand.swift` |

Design choices that keep it light:

- The menu bar item is AppKit; the menu is rebuilt only when opened.
- Windows are SwiftUI hosted in AppKit, created on demand and released on
  close.
- The Swift runtime and SwiftUI ship with macOS, so the app carries only its
  own code: about 1.3 MB per architecture.
- Engine callbacks arrive on engine threads; UI updates are coalesced into
  one main-thread refresh.

## Adding a platform

A new shell needs to:

1. capture local input and call `ew_engine_local_event` for each event,
   swallowing it when the core says so;
2. freeze/hide and restore the local cursor on the `control` callback;
3. post events from the `inject` callback (HID usages → native key codes);
4. report displays with `ew_engine_set_screens`;
5. provide a DNS-SD backend in `core/src/discovery.rs`;
6. render the snapshot JSON as a native interface.

See [platform/windows/README.md](../platform/windows/README.md) and
[platform/linux/README.md](../platform/linux/README.md).
