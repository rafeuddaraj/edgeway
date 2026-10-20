#!/usr/bin/env bash
#
# Builds Edgeway.app into build/ and a zip into dist/.
#
#   scripts/build-macos.sh                  for this Mac (Apple silicon or Intel)
#   scripts/build-macos.sh --arch arm64     Apple silicon only   (~1.4 MB app)
#   scripts/build-macos.sh --arch x86_64    Intel only           (~1.5 MB app)
#   scripts/build-macos.sh --arch universal both in one app      (~2.8 MB app)
#
# Needs: Xcode Command Line Tools and Rust (https://rustup.rs). Xcode itself
# is not required. Set EDGEWAY_SIGN_IDENTITY to a Developer ID to sign for
# distribution; by default the app is signed ad hoc for local use.
#
# Copyright (c) 2026 Rafe Uddaraj. MIT License.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FLAVOR="$(uname -m)"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) FLAVOR="${2:-}"; shift 2 ;;
    *) echo "unknown option: $1 (use --arch arm64|x86_64|universal)" >&2; exit 2 ;;
  esac
done
case "$FLAVOR" in
  arm64) ARCHS=(arm64); LABEL=apple-silicon ;;
  x86_64) ARCHS=(x86_64); LABEL=intel ;;
  universal) ARCHS=(arm64 x86_64); LABEL=universal ;;
  *) echo "unknown architecture: $FLAVOR" >&2; exit 2 ;;
esac

[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
command -v cargo >/dev/null || { echo "Rust is required: https://rustup.rs" >&2; exit 1; }

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' core/Cargo.toml | head -1)"
BUILD_NUMBER="$(git rev-list --count HEAD 2>/dev/null || echo 1)"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
OUT="$ROOT/build"
OBJ="$OUT/obj"
APP="$OUT/Edgeway.app"
export MACOSX_DEPLOYMENT_TARGET=13.0

rust_target() {
  case "$1" in
    arm64) echo aarch64-apple-darwin ;;
    x86_64) echo x86_64-apple-darwin ;;
  esac
}

step() { printf '\n\033[1m▸ %s\033[0m\n' "$1"; }

step "Core (Rust) for ${ARCHS[*]}"
libs=()
for arch in "${ARCHS[@]}"; do
  target="$(rust_target "$arch")"
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build --quiet --release --target "$target" --manifest-path core/Cargo.toml
  libs+=("core/target/$target/release/libedgeway_core.a")
done
mkdir -p core/target/universal-release
lipo -create "${libs[@]}" -output core/target/universal-release/libedgeway_core.a

step "App (Swift)"
mkdir -p "$OBJ"
sources=()
while IFS= read -r f; do sources+=("$f"); done < <(find platform/macos/Sources/Edgeway -name '*.swift' | sort)
bins=()
for arch in "${ARCHS[@]}"; do
  swiftc -Osize -wmo \
    -target "$arch-apple-macos13.0" -sdk "$SDK" \
    -I platform/macos/Sources/CEdgeway \
    -L core/target/universal-release -ledgeway_core \
    -Xlinker -dead_strip \
    "${sources[@]}" -o "$OBJ/Edgeway-$arch"
  bins+=("$OBJ/Edgeway-$arch")
done

step "Bundle"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${bins[@]}" -output "$APP/Contents/MacOS/Edgeway"
strip -x "$APP/Contents/MacOS/Edgeway"
sed -e "s/__VERSION__/$VERSION/" -e "s/__BUILD__/$BUILD_NUMBER/" \
  platform/macos/Resources/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

if [[ -f assets/brand/logo.png ]]; then
  swiftc -O scripts/make-icon.swift -o "$OBJ/make-icon" 2>/dev/null
  rm -rf "$OBJ/AppIcon.iconset"
  "$OBJ/make-icon" assets/brand/logo.png "$OBJ/AppIcon.iconset" assets/brand/edgeway-icon.png >/dev/null
  iconutil -c icns "$OBJ/AppIcon.iconset" -o "$APP/Contents/Resources/AppIcon.icns"
else
  echo "note: assets/brand/logo.png not found; building without an app icon" >&2
fi

step "Sign"
codesign --force --options runtime --sign "${EDGEWAY_SIGN_IDENTITY:--}" "$APP"
codesign --verify --strict "$APP"

step "Package"
mkdir -p "$ROOT/dist"
ZIP="$ROOT/dist/Edgeway-$VERSION-$LABEL.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

bytes() { stat -f %z "$1"; }
human() { awk -v b="$1" 'BEGIN { printf "%.2f MB (%d bytes)", b / 1048576, b }'; }
app_bytes="$(find "$APP" -type f -exec stat -f %z {} + | awk '{ s += $1 } END { print s }')"
echo
echo "Edgeway $VERSION ($BUILD_NUMBER) — ${ARCHS[*]}"
echo "  app bundle   $(human "$app_bytes")"
echo "  executable   $(human "$(bytes "$APP/Contents/MacOS/Edgeway")")"
echo "  download     $(human "$(bytes "$ZIP")")  → ${ZIP#$ROOT/}"
