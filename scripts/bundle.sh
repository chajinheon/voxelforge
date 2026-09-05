#!/bin/bash
set -euo pipefail

ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
DIST="$ROOT/target/dist"
APP="$DIST/Voxelforge.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
ICONSET="$DIST/Voxelforge.iconset"
ICON_PNG="$DIST/voxelforge-icon.png"

mkdir -p "$DIST"
cargo build --manifest-path "$ROOT/Cargo.toml" --release --bin voxelforge --bin icon
cargo run --manifest-path "$ROOT/Cargo.toml" --quiet --release --bin icon -- "$ICON_PNG"

rm -rf "$APP" "$ICONSET"
install -d "$MACOS" "$RESOURCES" "$ICONSET"
install -m 644 "$ROOT/assets/macos/Info.plist" "$CONTENTS/Info.plist"
install -m 755 "$ROOT/target/release/voxelforge" "$MACOS/voxelforge-bin"
ditto "$ROOT/assets" "$RESOURCES/assets"

cat > "$MACOS/voxelforge-launcher" <<'LAUNCHER'
#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
RESOURCES="$HERE/../Resources"
SUPPORT="${HOME}/Library/Application Support/Voxelforge"

mkdir -p "$SUPPORT/saves" "$SUPPORT/shaderpacks"
mkdir -p "${HOME}/Pictures/Voxelforge"

export VF_ASSETS="$RESOURCES/assets"
export VF_SAVE_ROOT="$SUPPORT/saves"
export VF_SETTINGS="$SUPPORT/settings.json"
export VF_SHADERPACKS="$SUPPORT/shaderpacks"
export VF_SCREENSHOTS="${HOME}/Pictures/Voxelforge"

exec "$HERE/voxelforge-bin" "$@"
LAUNCHER
chmod 755 "$MACOS/voxelforge-launcher"

for size in 16 32 64 128 256 512 1024; do
    sips -z "$size" "$size" "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$RESOURCES/Voxelforge.icns"
rm -rf "$ICONSET" "$ICON_PNG"

identity="${VF_CODESIGN_IDENTITY:--}"
if [[ "$identity" == "-" ]]; then
    codesign --force --sign - "$MACOS/voxelforge-bin"
    codesign --force --sign - "$MACOS/voxelforge-launcher"
    codesign --force --sign - "$APP"
else
    codesign --force --options runtime --timestamp --sign "$identity" "$MACOS/voxelforge-bin"
    codesign --force --options runtime --timestamp --sign "$identity" "$MACOS/voxelforge-launcher"
    codesign --force --options runtime --timestamp --sign "$identity" "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"
echo "bundle: $APP"
