# Security

Edgeway turns network packets into keystrokes, so it is designed on the
assumption that the local network is hostile.

## Guarantees

1. **Nothing is accepted from a device you haven't paired.** The input
   injection path is reachable only after a peer has proved, in the
   handshake, that it holds the private key of a device you paired with. An
   unpaired device can do exactly one thing: ask to pair. The receiving device
   then shows a code on its own screen. Covered by the
   `an_unpaired_device_can_never_inject_input` test.
2. **Everything is encrypted and authenticated.** Every byte after the first
   handshake message is protected with ChaCha20-Poly1305. Tampering,
   replaying or reordering a message breaks the connection.
3. **Forward secrecy.** Session keys come from fresh ephemeral keys. Stealing
   a device's long-term key later doesn't decrypt recorded sessions.
4. **No accounts, servers, telemetry or analytics.** Edgeway talks only to
   devices on your local network.

## How it works

### Identity

Each installation creates an X25519 key pair on first launch and stores it in
`~/Library/Application Support/Edgeway/identity.key`, readable only by your
user (mode 0600, like an SSH key). The public key *is* the device's identity.
DNS-SD advertises a 64-bit fingerprint of it so a paired device can be
recognised before connecting; the fingerprint is only a hint and is never
trusted on its own.

### Transport: Noise XX

Every connection runs `Noise_XX_25519_ChaChaPoly_BLAKE2s` with the prologue
`edgeway/1` (binding the handshake to protocol version 1), implemented by the
widely used [`snow`](https://github.com/mcginty/snow) crate. Both sides learn
each other's static public key, encrypted under the ephemeral exchange.
Afterwards:

- If the remote key is **trusted**, the session starts.
- If it is **not trusted**, the only accepted message is a pairing request.
  Anything else closes the connection.
- When dialing a trusted device, a different key than the one paired is
  rejected ("This device has a new identity").

### Pairing: six digits, SPAKE2, channel binding

1. The controlling device sends a pairing request.
2. The device being controlled generates a uniformly random six-digit code,
   shows it on **its own screen**, and starts SPAKE2 with it.
3. The person types the code on the controlling device, which completes
   SPAKE2. **The code itself never travels over the network.**
4. Both sides prove they derived the same key with BLAKE2s MACs over the Noise
   handshake hash and both public keys; the initiator proves first.
5. Each side stores the other's public key.

Why this resists attack:

- A **passive** observer learns nothing about the code or the key.
- A **machine in the middle** must run two separate Noise sessions with two
  different handshake hashes. Even if it relays every SPAKE2 message, the key
  confirmations are bound to the hash and fail.
- An **active guesser** gets one guess per attempt (one in a million). After
  five wrong codes within ten minutes the device refuses further pairing for
  the rest of that window, and every attempt shows a fresh code on screen.
- Physical presence is required: the code is only ever visible on the
  controlled computer's own display.

### Availability and safety

- Unauthenticated connections are limited (8 at a time) and must finish the
  handshake within 5 seconds and request pairing within 30.
- Incoming messages are size-checked: at most 16 screens, finite coordinates,
  bounded strings and buttons.
- Heartbeats run every second while connected. After 3.5 s of silence the
  controlled device ends the session and releases every key and button it
  holds, so a dropped connection can't leave a key stuck down.
- The controlling device returns control home on disconnect, sleep, screen
  lock, Secure Input, or when macOS interrupts its event tap.
- On macOS, `SO_NOSIGPIPE` keeps a closed socket from terminating the app.

### What is stored

| File | Contents |
|---|---|
| `identity.key` | This device's private and public key (0600) |
| `config.json` | Paired devices' public keys and names, their display sizes and arrangement, the listening port, edge resistance |

Keystrokes, pointer data and screen contents are never stored or logged.

## Known limitations

- The private key is protected by file permissions, not the Keychain or Secure
  Enclave. Anyone who can read files as your user can copy it, as with SSH
  keys. Moving it to the Keychain for signed releases is planned.
- The Noise implementation does not claim protection against timing
  side-channels on the local machine.
- Release builds are not yet notarized by Apple; see
  [INSTALL.md](INSTALL.md#edgeway-cant-be-opened--and-how-to-never-see-it-again).

## Reporting a vulnerability

Please report security problems privately through
[GitHub's security advisories](https://github.com/rafeuddaraj/edgeway/security/advisories/new)
rather than in a public issue. Include the version, the platform and steps to
reproduce. You'll get an acknowledgement within a week.
