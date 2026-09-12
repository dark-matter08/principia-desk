#!/usr/bin/env bash
# Build a DMG from a fresh Tauri .app bundle using macOS system tools.
# Explicit sizing avoids hdiutil's undersized-image failure on macOS 15.
set -euo pipefail
cd "$(dirname "$0")/.."

BUNDLE_DIR="${PRINCIPIA_BUNDLE_DIR:-src-tauri/target/release/bundle}"
PRODUCT_NAME="$(node -p "require('./src-tauri/tauri.conf.json').productName")"
VERSION="$(node -p "require('./package.json').version")"
APP_PATH="$BUNDLE_DIR/macos/$PRODUCT_NAME.app"
DMG_DIR="$BUNDLE_DIR/dmg"
ARCH="${PRINCIPIA_DMG_ARCH:-$(uname -m)}"
DMG="$DMG_DIR/${PRODUCT_NAME}_${VERSION}_${ARCH}.dmg"

if [ ! -d "$APP_PATH" ]; then
  echo "App bundle missing: $APP_PATH. Run npm run tauri build -- --bundles app first." >&2
  exit 1
fi

mkdir -p "$DMG_DIR"
STAGING="$(mktemp -d "$DMG_DIR/.principia-dmg.XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT
mkdir -p "$STAGING/payload"
ditto "$APP_PATH" "$STAGING/payload/$PRODUCT_NAME.app"
ln -s /Applications "$STAGING/payload/Applications"
APP_KIB="$(du -sk "$STAGING/payload" | awk '{print $1}')"
IMAGE_MIB=$((APP_KIB * 12 / 10 / 1024 + 64))
hdiutil create -quiet -size "${IMAGE_MIB}m" -fs HFS+ \
  -volname "$PRODUCT_NAME" -srcfolder "$STAGING/payload" \
  -format UDZO "$STAGING/output.dmg"
hdiutil verify "$STAGING/output.dmg"
mv -f "$STAGING/output.dmg" "$DMG"
echo "built $DMG"
