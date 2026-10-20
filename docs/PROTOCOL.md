# Edgeway protocol, version 1

This document is the contract between Edgeway implementations on different
platforms. The reference implementation is `core/src/protocol.rs`,
`core/src/noise.rs` and `core/src/pairing.rs`.

## Discovery

The controlled device advertises a DNS-SD service:

| Field | Value |
|---|---|
| Service type | `_edgeway._tcp` |
| Instance name | The device's display name, e.g. `Mac mini` |
| Port | Listening TCP port (default 47820, otherwise the next free one) |
| TXT `v` | Protocol version, `1` |
| TXT `id` | 16 lowercase hex characters: the first 8 bytes of BLAKE2s-256(public key) |
| TXT `p` | `macos`, `windows` or `linux` |

The controlling device browses for this type. TXT data is never trusted; it
only helps a controlling device recognise a paired device before connecting.

## Transport

TCP with `TCP_NODELAY`, DSCP EF (`0xB8`) as a hint for low-latency Wi-Fi
queuing.

Every message, including handshake messages, is a **frame**:

```
u16 length (big-endian) | length bytes
```

### Handshake

`Noise_XX_25519_ChaChaPoly_BLAKE2s`, prologue the ASCII bytes `edgeway/1`.
The connecting device is the initiator. Handshake payloads are empty.

```
-> e
<- e, ee, s, es
-> s, se
```

After the handshake each frame carries one ciphertext produced with the
Noise transport cipher, using implicit, strictly increasing nonces per
direction (0, 1, 2…). The plaintext is one message.

### Messages

Plaintext layout: a 1-byte tag followed by fields. Integers and floats are
**little-endian**. `str` and `bytes` are a `u16` length then the data
(strings are UTF-8, at most 1024 bytes). `f32` values must be finite.
Unknown tags must be ignored. Extra trailing bytes must be ignored, so fields
can be appended in later versions.

| Tag | Message | Fields | Direction |
|---|---|---|---|
| `0x01` | Hello | `u16 version`, `str name`, `u8 platform`, `screens` | both, first after auth |
| `0x02` | Screens | `screens` | both |
| `0x08` | PairRequest | `str name`, `u8 platform` | I → R |
| `0x09` | PairChallenge | `bytes spake` | R → I |
| `0x0A` | PairResponse | `bytes spake`, `[32] confirm` | I → R |
| `0x0B` | PairConfirm | `[32] confirm` | R → I |
| `0x0C` | PairRejected | `u8 reason` (1 denied, 2 wrong code, 3 busy, 4 too many attempts, 5 timeout) | R → I |
| `0x10` | Enter | `f32 x`, `f32 y` | controller → controlled |
| `0x11` | Leave | — | controller → controlled |
| `0x12` | MouseMove | `f32 x`, `f32 y`, `f32 dx`, `f32 dy` | controller → controlled |
| `0x13` | MouseButton | `u8 button`, `u8 down`, `u8 clicks` | controller → controlled |
| `0x14` | Scroll | `f32 dx`, `f32 dy`, `i16 lines_x`, `i16 lines_y`, `u8 phase`, `u8 momentum`, `u8 continuous` | controller → controlled |
| `0x15` | Key | `u16 key`, `u8 down`, `u8 repeat` | controller → controlled |
| `0x16` | ReleaseAll | — | controller → controlled |
| `0x20` | Ping | `u64 t` | both |
| `0x21` | Pong | `u64 t` (echoed) | both |
| `0x2F` | Bye | — | both |

`screens` is `u8 count` (≤ 16) followed by `count` × (`f32 x`, `f32 y`,
`f32 w`, `f32 h`, `str name`), in the sender's global coordinates: points,
origin at the top-left of its main display, y growing downward. Widths and
heights must be between 1 and 100,000.

`platform`: 0 unknown, 1 macOS, 2 Windows, 3 Linux.

`button`: 0 left, 1 right, 2 middle, 3 back, 4 forward, ≤ 31.

Scroll `phase` and `momentum`: 0 none, 1 began, 2 changed, 3 ended,
4 cancelled, 5 may begin. `dx`/`dy` are pixels (continuous devices);
`lines_*` are wheel detents.

`key` is a USB HID usage from the Keyboard/Keypad page (`0x04` = A,
`0xE0`–`0xE7` = modifiers). `0x0300` is the Apple Fn/Globe key. Modifier
state is derived by the receiver from the keys it holds; `Key` events for
modifiers are sent like any other key.

Positions in `Enter` and `MouseMove` are absolute, in the **receiver's**
coordinates. The controller computes them from its arrangement.

### Session rules

1. After the handshake, if the responder does not trust the initiator's
   static key, the only acceptable message is `PairRequest`. Anything else
   closes the connection.
2. Both sides send `Hello`. A different `version` ends the session.
3. Both sides send `Ping` every second; any received frame counts as
   activity. After 3.5 s without activity the session ends.
4. When a session ends for any reason, the controlled device releases every
   key and button it is holding.
5. A controlled device serves one controller at a time; a new authenticated
   session replaces the previous one.

## Pairing

SPAKE2 over the Ed25519 group (`spake2` crate, version 0.4), with identities
`edgeway/1 pairing initiator` (A) and `edgeway/1 pairing responder` (B), and
the six ASCII digits of the code as the password.

```
I → R  PairRequest(name, platform)
R      code = random 000000–999999, shown on R's screen
R      (state_b, msg_b) = SPAKE2.start_b(code)
R → I  PairChallenge(msg_b)
I      person types code; (state_a, msg_a) = SPAKE2.start_a(code); K = finish(msg_b)
I → R  PairResponse(msg_a, Ti = MAC(K, "edgeway/1 confirm initiator" ‖ h ‖ pk_I ‖ pk_R))
R      K = finish(msg_a); verify Ti, else PairRejected(2)
R → I  PairConfirm(Tr = MAC(K, "edgeway/1 confirm responder" ‖ h ‖ pk_I ‖ pk_R))
I      verify Tr
```

`MAC` is keyed BLAKE2s-256 with K as the key; `h` is the 32-byte Noise
handshake hash; `pk` are the static public keys. On success both sides store
the other's public key and continue with `Hello` on the same connection.

The responder allows at most five failed attempts in ten minutes.
