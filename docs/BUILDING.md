# Building Edgeway

- [macOS](#macos)
  - [1. Install the tools](#1-install-the-tools)
  - [2. Get the source](#2-get-the-source)
  - [3. Build](#3-build)
  - [4. Install and run your build](#4-install-and-run-your-build)
  - [Keep permissions across rebuilds](#keep-permissions-across-rebuilds)
  - [Signing and notarizing a release](#signing-and-notarizing-a-release)
  - [Day-to-day development](#day-to-day-development)
  - [Troubleshooting](#troubleshooting)
- [Windows and Linux](#windows-and-linux)

---

## macOS

You need about 2 GB of free space for the toolchains, and no Xcode. The Xcode
Command Line Tools are enough.

### 1. Install the tools

```bash
xcode-select --install
```

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
```

```bash
source "$HOME/.cargo/env"
```

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

Check that everything is there:

```bash
swiftc --version && cargo --version
```

### 2. Get the source

```bash
git clone https://github.com/rafeuddaraj/edgeway.git
```

```bash
cd edgeway
```

### 3. Build

For the Mac you are on (fastest):

```bash
scripts/build-macos.sh
```

For a specific architecture:

```bash
scripts/build-macos.sh --arch arm64
```

```bash
scripts/build-macos.sh --arch x86_64
```

```bash
scripts/build-macos.sh --arch universal
```

The script:

1. builds the Rust core (`core/`) as a static library, size-optimised;
2. compiles the Swift app directly with `swiftc` (`platform/macos/`);
3. assembles `build/Edgeway.app`, generating the app icon from
   `assets/brand/logo.png`;
4. signs it (see below) and writes `dist/Edgeway-<version>-<arch>.zip`;
5. prints the real size of the app and the download.

Typical output on an M1:

```
Edgeway 0.1.0 (3) — arm64
  app bundle   1.32 MB (1382268 bytes)
  executable   1.31 MB (1378592 bytes)
  download     0.55 MB (572526 bytes)  → dist/Edgeway-0.1.0-apple-silicon.zip
```

### 4. Install and run your build

```bash
scripts/install.sh dist/Edgeway-0.1.0-apple-silicon.zip
```

Or run it straight from the build folder with logs in your terminal:

```bash
build/Edgeway.app/Contents/MacOS/Edgeway
```

Apps you build yourself are never quarantined, so the Gatekeeper warning
described in [INSTALL.md](INSTALL.md) doesn't apply to them. If you copy
your build to another Mac through AirDrop, a browser or chat, run the
installer there, or clear the flag once:

```bash
xattr -dr com.apple.quarantine /Applications/Edgeway.app
```

### Keep permissions across rebuilds

macOS remembers the Accessibility permission by the app's code signature.
Without a signing certificate the build script signs *ad hoc*, which gives
every build a different signature, so macOS asks for Accessibility again
after each rebuild. Fix this once per Mac:

```bash
scripts/create-signing-identity.sh
```

It creates a private, self-signed "Edgeway Local Signing" certificate in your
login keychain (macOS asks for your password once to trust it). The build
script uses it automatically from then on. Rebuild, grant Accessibility one
last time, and it stays granted through every later build.

If macOS ever shows Edgeway as allowed but it still can't control input
(this happens after switching from ad hoc to a certificate), reset the entry
and allow it again:

```bash
tccutil reset Accessibility io.github.rafeuddaraj.Edgeway
```

### Signing and notarizing a release

Public downloads open without any warning only when they are signed with an
Apple **Developer ID** and notarized (Apple Developer Program, US$99/year).

```bash
EDGEWAY_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" scripts/build-macos.sh --arch arm64
```

```bash
xcrun notarytool submit dist/Edgeway-0.1.0-apple-silicon.zip --keychain-profile edgeway --wait
```

```bash
xcrun stapler staple build/Edgeway.app
```

Re-zip after stapling (`ditto -c -k --keepParent build/Edgeway.app dist/…zip`).
Create the `edgeway` keychain profile once with
`xcrun notarytool store-credentials edgeway`. `notarytool` and `stapler`
come with the Command Line Tools. Releases should be built for `arm64` and
`x86_64` separately so each download stays around half a megabyte.

### Day-to-day development

Run the core's tests (unit tests plus end-to-end tests that pair two engines
over loopback, route input, drop the connection and reconnect):

```bash
cd core && cargo test
```

Run two headless peers with real Bonjour discovery, without any UI or input
permissions. Received input is printed instead of injected:

```bash
cd core && cargo run --release --example peer -- secondary /tmp/ew-s --code 482913
```

```bash
cd core && cargo run --release --example peer -- primary /tmp/ew-p --code 482913 --type hello
```

Run two copies of the app on one Mac. Each `--profile` has its own identity,
pairings and settings. `--dry-run` logs received input instead of posting it:

```bash
build/Edgeway.app/Contents/MacOS/Edgeway --profile mini --role secondary --dry-run
```

```bash
build/Edgeway.app/Contents/MacOS/Edgeway --profile book --role primary
```

Render every window with sample data, in light and dark, to
`build/previews/` (no permissions needed, nothing appears on screen):

```bash
scripts/render-previews.sh
```

Open the Swift sources in Xcode, if you have it, through
`platform/macos/Package.swift` after building the core once. Set
`EDGEWAY_QUIET=1` to silence the core's log lines.

### Troubleshooting

| Problem | Fix |
|---|---|
| `xcrun: error: invalid active developer path` | Run `xcode-select --install`. |
| `Rust is required` | Install Rust (step 1) and open a new terminal, or run `source "$HOME/.cargo/env"`. |
| `error: target … not installed` | `rustup target add aarch64-apple-darwin x86_64-apple-darwin` |
| `swift build` fails with "Invalid manifest" or undefined `PackageDescription` symbols | A known mismatch in some Command Line Tools installs. Use `scripts/build-macos.sh`, which calls `swiftc` directly and doesn't depend on SwiftPM. |
| Edgeway asks for Accessibility after every build | Run `scripts/create-signing-identity.sh` once. |
| The other Mac never appears under Nearby | Both Macs must be on the same network; allow Local Network for Edgeway in System Settings › Privacy & Security › Local Network; guest or "client isolation" Wi-Fi networks block devices from seeing each other. |

---

## Windows and Linux

The core (`core/`) is portable: it compiles for Windows and Linux today, and
CI runs its tests on macOS and Linux:

```bash
cd core && cargo test
```

The Windows and Linux shells, which add input capture, injection, discovery
backends and the native interface, are planned. Their designs are in
[platform/windows/README.md](../platform/windows/README.md) and
[platform/linux/README.md](../platform/linux/README.md). Build instructions
will be added here with them.
