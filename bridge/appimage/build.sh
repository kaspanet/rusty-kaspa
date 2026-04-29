#!/usr/bin/env bash
# Build stratum-bridge AppImage from an existing musl release binary.
# Usage: from repo root, after `cargo build --bin stratum-bridge --release --target x86_64-unknown-linux-musl`:
#   bash bridge/appimage/build.sh [version-label]
set -euo pipefail

VERSION="${1:-dev}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PACK_DIR="${SCRIPT_DIR}"
cd "$ROOT"

BINARY="target/x86_64-unknown-linux-musl/release/stratum-bridge"
if [[ ! -f "$BINARY" ]]; then
  echo "error: missing $BINARY — build stratum-bridge for x86_64-unknown-linux-musl first." >&2
  exit 1
fi

APPDIR="${PACK_DIR}/StratumBridge.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
cp "$BINARY" "$APPDIR/usr/bin/stratum-bridge"
chmod +x "$APPDIR/usr/bin/stratum-bridge"

cp "${PACK_DIR}/AppRun" "$APPDIR/AppRun"
chmod +x "$APPDIR/AppRun"

mkdir -p "$APPDIR/usr/share/applications"
cp "${PACK_DIR}/stratum-bridge.desktop" "$APPDIR/usr/share/applications/stratum-bridge.desktop"
# appimagetool requires exactly one .desktop at the AppDir root (may be a symlink).
ln -sf "usr/share/applications/stratum-bridge.desktop" "${APPDIR}/stratum-bridge.desktop"

ICON_DIR="$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$ICON_DIR"
SVG="${ROOT}/bridge/static/assets/kaspa.svg"
BUNDLED_PNG="${PACK_DIR}/stratum-bridge.png"
# appimagetool requires Icon=name as name.png at the AppDir root (256x256 recommended).
if [[ -f "$SVG" ]] && command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 256 -h 256 "$SVG" -o "${ICON_DIR}/stratum-bridge.png"
elif [[ -f "$BUNDLED_PNG" ]]; then
  cp "$BUNDLED_PNG" "${ICON_DIR}/stratum-bridge.png"
elif [[ -f "$SVG" ]]; then
  echo "error: rsvg-convert not found and no ${BUNDLED_PNG}; cannot produce stratum-bridge.png for AppImage." >&2
  exit 1
else
  echo "error: missing kaspa.svg and bundled stratum-bridge.png; cannot produce app icon." >&2
  exit 1
fi
cp "${ICON_DIR}/stratum-bridge.png" "${APPDIR}/stratum-bridge.png"
cp "${ICON_DIR}/stratum-bridge.png" "${APPDIR}/.DirIcon"

TOOL="${PACK_DIR}/appimagetool-x86_64.AppImage"
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
