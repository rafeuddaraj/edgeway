#!/usr/bin/env bash
#
# Installs (or updates) Edgeway on macOS without Gatekeeper prompts.
#
#   curl -fsSL https://raw.githubusercontent.com/rafeuddaraj/edgeway/main/scripts/install.sh | bash
#   scripts/install.sh dist/Edgeway-0.1.0-apple-silicon.zip     # a zip you built or downloaded
#
# Why no prompt: macOS attaches a "quarantine" flag to files downloaded by a
# browser, and Gatekeeper blocks quarantined apps that are not notarized by
# Apple. curl does not set that flag, and this script clears it in case the
# zip came from a browser. Use the same command for every update and you will
# never see the "cannot be opened" warning again.
#
# It installs to /Applications (or ~/Applications if that is not writable),
# quits a running Edgeway first and opens the new one. Your pairings and
# settings are kept.
#
# Environment: EDGEWAY_INSTALL_DIR (default /Applications), EDGEWAY_NO_OPEN=1,
# EDGEWAY_REPO (default rafeuddaraj/edgeway).
#
# Copyright (c) 2026 Rafe Uddaraj. MIT License.

set -euo pipefail

REPO="${EDGEWAY_REPO:-rafeuddaraj/edgeway}"
APP_NAME="Edgeway.app"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "this installer is for macOS"
major="$(sw_vers -productVersion | cut -d. -f1)"
(( major >= 13 )) || die "Edgeway needs macOS 13 Ventura or later"

case "$(uname -m)" in
  arm64) flavor=apple-silicon ;;
  x86_64) flavor=intel ;;
  *) die "unsupported Mac architecture: $(uname -m)" ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [[ $# -ge 1 ]]; then
  zip="$1"
  [[ -f "$zip" ]] || die "no such file: $zip"
else
  say "Finding the latest Edgeway release…"
  api="https://api.github.com/repos/$REPO/releases/latest"
  url="$(curl -fsSL "$api" | grep -o "\"browser_download_url\": *\"[^\"]*-$flavor\.zip\"" | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')"
  [[ -n "$url" ]] || die "no $flavor download found in the latest release of $REPO"
  say "Downloading ${url##*/}…"
  zip="$work/${url##*/}"
  curl -fL --progress-bar "$url" -o "$zip"
fi

ditto -x -k "$zip" "$work/unpacked"
[[ -d "$work/unpacked/$APP_NAME" ]] || die "the archive does not contain $APP_NAME"

if [[ -n "${EDGEWAY_INSTALL_DIR:-}" ]]; then
  dest="$EDGEWAY_INSTALL_DIR"
  mkdir -p "$dest"
else
  dest="/Applications"
  [[ -w "$dest" ]] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
fi

if [[ -z "${EDGEWAY_NO_OPEN:-}" ]] && pgrep -x Edgeway >/dev/null; then
  say "Quitting the running Edgeway…"
  osascript -e 'tell application id "io.github.rafeuddaraj.Edgeway" to quit' >/dev/null 2>&1 || pkill -x Edgeway || true
  sleep 1
fi

say "Installing to $dest…"
rm -rf "${dest:?}/$APP_NAME"
ditto "$work/unpacked/$APP_NAME" "$dest/$APP_NAME"
xattr -dr com.apple.quarantine "$dest/$APP_NAME" 2>/dev/null || true

if [[ -z "${EDGEWAY_NO_OPEN:-}" ]]; then
  say "Opening Edgeway…"
  open "$dest/$APP_NAME"
fi
say "Done. Edgeway is in your menu bar."
