#!/bin/bash
set -euo pipefail

if [[ -z "${VF_NOTARY_PROFILE:-}" ]]; then
    echo "VF_NOTARY_PROFILE is required (Keychain profile for xcrun notarytool)" >&2
    exit 2
fi

ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
APP="${VF_APP_PATH:-$ROOT/target/dist/Voxelforge.app}"
ZIP="${VF_NOTARY_ZIP:-$ROOT/target/dist/Voxelforge-notarize.zip}"

if [[ ! -d "$APP" ]]; then
    echo "bundle not found: $APP" >&2
    exit 1
fi

rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$VF_NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
echo "notarized: $APP"
