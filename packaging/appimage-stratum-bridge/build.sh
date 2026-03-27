#!/usr/bin/env bash
# Build stratum-bridge AppImage from an existing musl release binary.
# Usage: from repo root, after `cargo build --bin stratum-bridge --release --target x86_64-unknown-linux-musl`:
#   bash packaging/appimage-stratum-bridge/build.sh [version-label]
set -euo pipefail

VERSION="${1:-dev}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BINARY="target/x86_64-unknown-linux-musl/release/stratum-bridge"
if [[ ! -f "$BINARY" ]]; then
  echo "error: missing $BINARY — build stratum-bridge for x86_64-unknown-linux-musl first." >&2
  exit 1
fi

APPDIR="${ROOT}/packaging/appimage-stratum-bridge/StratumBridge.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
cp "$BINARY" "$APPDIR/usr/bin/stratum-bridge"
chmod +x "$APPDIR/usr/bin/stratum-bridge"

cp "${ROOT}/packaging/appimage-stratum-bridge/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"

mkdir -p "$APPDIR/usr/share/applications"
cp "${ROOT}/packaging/appimage-stratum-bridge/stratum-bridge.desktop" "$APPDIR/usr/share/applications/stratum-bridge.desktop"

ICON_DIR="$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$ICON_DIR"
SVG="${ROOT}/bridge/static/assets/kaspa.svg"
if [[ -f "$SVG" ]] && command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 256 -h 256 "$SVG" -o "${ICON_DIR}/stratum-bridge.png"
  cp "${ICON_DIR}/stratum-bridge.png" "$APPDIR/.DirIcon"
elif [[ -f "$SVG" ]]; then
  echo "warning: rsvg-convert not found; install librsvg2-bin for a PNG icon (AppImage still builds)." >&2
fi

TOOL="${ROOT}/packaging/appimage-stratum-bridge/appimagetool-x86_64.AppImage"
if [[ ! -x "$TOOL" ]]; then
  echo "Downloading appimagetool..."
  wget -qO "$TOOL" "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage"
  chmod +x "$TOOL"
fi

export ARCH=x86_64
# Allow appimagetool to run without FUSE (e.g. GitHub Actions, CI).
export APPIMAGE_EXTRACT_AND_RUN=1
OUT="${ROOT}/stratum-bridge-${VERSION}-x86_64.AppImage"
rm -f "$OUT"
"$TOOL" "$APPDIR" "$OUT"
echo "Built: $OUT"
