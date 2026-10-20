# Testing

## Automated

```bash
cd core && cargo test
```

| Suite | What it proves |
|---|---|
| `wire`, `protocol` | Every message round-trips; truncated, non-finite or absurd input is rejected; unknown messages are tolerated |
| `noise` | Both sides learn each other's identity and the same handshake hash; tampering and replay are detected |
| `pairing` | Right code confirms; wrong code fails; a relayed (machine-in-the-middle) exchange fails even with the right code; throttling |
| `identity`, `config` | Keys persist with 0600 permissions; config survives missing fields and corruption |
| `keymap` | The macOS ↔ HID table is a bijection; modifiers map correctly |
| `switching` | Crossing only after the resistance; push resets; clamping; returning to the matching point; everything swallowed while away; no crossing while a button is held; keys pressed before crossing are released locally; ⌃⌥⌘H; peer loss; pause |
| `keystate`, `injector` | Releases follow presses across switches; `ReleaseAll` order; stray releases ignored; Caps Lock; display-wake rate limiting |
| `tests/engine.rs` | Two engines on loopback: pair with the code, default placement above, cross, type, **drop the controller while a key is held → the key is released**, restart → **reconnect without pairing**, lose the controlled device → cursor comes home; wrong code → nothing trusted; **an unpaired device that skips pairing can't inject anything** |

## Measured on this machine

M1 MacBook Pro, 8 GB, macOS 26.5.1, release build, 20 October 2026.

| Measurement | Result | How |
|---|---|---|
| App bundle, Apple silicon | 1.32 MB (1,382,268 bytes) | `scripts/build-macos.sh` |
| Download, Apple silicon | 0.55 MB | same |
| App bundle, universal | 2.72 MB | `--arch universal` |
| Idle memory footprint (each role) | 11 MB | `footprint <pid>` after start, no windows open |
| Idle CPU over 60 s | 0.00 s (controlled) / 0.02 s (controlling) | `ps -o time` before and after |
| Pairing, discovery to connected | 110 ms | `examples/peer`, real Bonjour, two processes |
| Reconnect after restart | 50 ms | same, second run |
| Heartbeat round trip | 0.28 ms | loopback |
| Key held, controller frozen (`kill -STOP`) | released by the controlled side ≈4 s later | `examples/peer --hold` |

Re-run these with:

```bash
cd core && cargo run --release --example peer -- secondary /tmp/ew-s --code 482913
```

```bash
cd core && cargo run --release --example peer -- primary /tmp/ew-p --code 482913 --type hello
```

## Needs two physical Macs

These depend on real input hardware, permissions and two screens, so they
can't run in CI. Please run through them when testing a release and report
the results (macOS versions, Wi-Fi or Ethernet, models).

**Setup**
- [ ] Both Macs show each other's state correctly in the menu (Connected, RTT).
- [ ] Pairing: code appears on the Mac mini's own screen; typing it on the
      MacBook pairs both; a wrong code is rejected on both.

**Switching**
- [ ] Pushing through the top edge moves the pointer to the Mac mini;
      reaching for the menu bar normally does not.
- [ ] While on the Mac mini, nothing happens on the MacBook: no cursor, no
      clicks, no typing, no scrolling, no gestures, no hot corners.
- [ ] Coming back down places the cursor at the matching point.
- [ ] Rearranging (left/right/below) in Arrange Displays changes the edge.

**Input**
- [ ] Typing, including fast typing and key repeat.
- [ ] Shortcuts: ⌘C/⌘V, ⌘Tab, ⌘Space, ⌃←/⌃→, ⇧⌘4.
- [ ] Holding ⌘ while crossing: the ⌘ release stays on the MacBook.
- [ ] Click, double-click, right-click, drag and drop, text selection.
- [ ] Two-finger scrolling is smooth, with momentum and rubber-banding.
- [ ] Caps Lock toggles on the Mac mini.

**Safety**
- [ ] ⌃⌥⌘H brings control back from anywhere.
- [ ] Focusing a password field on the MacBook while away brings control
      home with a notice.
- [ ] Turning Wi-Fi off on the MacBook while holding a key: the Mac mini
      releases it within about four seconds; the MacBook cursor comes back.
- [ ] Sleep and wake either Mac: they reconnect on their own.

**Restart**
- [ ] Restart the Mac mini with automatic login: the MacBook reconnects
      without anyone touching the Mac mini.
- [ ] With FileVault on (macOS 26, Remote Login on): after `ssh` unlock, note
      whether the Mac mini reaches the desktop or stops at the login window.
- [ ] Whether input works at the Mac mini's lock screen.

**Resources**
- [ ] Activity Monitor: Edgeway near 0 % CPU when idle; memory stays flat over
      a day.
