#!/usr/bin/env bash
#
# Renders Edgeway's windows with sample data to PNGs (light and dark), for
# design review and documentation. Needs the core built once
# (scripts/build-macos.sh).
#
#   scripts/render-previews.sh [output-directory]
#
# Copyright (c) 2026 Rafe Uddaraj. MIT License.

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-$ROOT/build/previews}"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
mkdir -p build/obj
sources=()
while IFS= read -r f; do sources+=("$f"); done < <(find platform/macos/Sources/Edgeway -name '*.swift' ! -name main.swift | sort)
swiftc -O -D EDGEWAY_PREVIEWS -target "$(uname -m)-apple-macos13.0" -sdk "$SDK" \
  -I platform/macos/Sources/CEdgeway -L core/target/universal-release -ledgeway_core \
  "${sources[@]}" platform/macos/Tools/Previews/main.swift -o build/obj/edgeway-previews
ICON="assets/brand/edgeway-icon.png"
build/obj/edgeway-previews "$OUT" ${ICON:+$( [[ -f $ICON ]] && echo "$ICON")}
