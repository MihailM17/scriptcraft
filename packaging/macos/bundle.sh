#!/usr/bin/env bash
# Build a macOS .app and DMG for ScriptCraft.
#
# Ad-hoc signing works without secrets and is fine for local testing:
#   packaging/macos/bundle.sh
#
# A distributable build needs a Developer ID Application certificate and a
# notarytool keychain profile:
#   SIGN_IDENTITY="Developer ID Application: Name (TEAMID)" \
#   NOTARY_PROFILE="scriptcraft-notary" packaging/macos/bundle.sh
#
# Without SIGN_IDENTITY the output is ad-hoc signed and NOT notarised; never
# publish it as a signed build.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/scriptcraft-app/Cargo.toml" | head -1)"
[ -n "$VERSION" ] || { echo "cannot read version" >&2; exit 1; }
ARCH="$(uname -m)"
case "$ARCH" in
  arm64) LABEL="macos-arm64" ;;
  x86_64) LABEL="macos-x64" ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

STAGE="$ROOT/dist/ScriptCraft-$VERSION-$LABEL"
APP="$STAGE/ScriptCraft.app"
DMG="$ROOT/dist/ScriptCraft-$VERSION-$LABEL.dmg"

echo "Building ScriptCraft $VERSION ($LABEL)"
cargo build --release --manifest-path "$ROOT/Cargo.toml" -p scriptcraft-app

rm -rf "$STAGE" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/scriptcraft-app" "$APP/Contents/MacOS/ScriptCraft"
cp "$ROOT/scriptcraft-app/assets/ScriptCraft.icns" "$APP/Contents/Resources/"
# Spellcheck dictionaries stay data files (their licences differ from the
# app's); `spell::dictionary_search_paths` finds them in Contents/Resources.
cp -R "$ROOT/scriptcraft-app/assets/dictionaries" "$APP/Contents/Resources/dictionaries"
sed "s/__VERSION__/$VERSION/g" "$ROOT/packaging/macos/Info.plist.in" \
  > "$APP/Contents/Info.plist"
ln -s /Applications "$STAGE/Applications"

if [ -n "${SIGN_IDENTITY:-}" ]; then
  echo "Signing with: $SIGN_IDENTITY"
  codesign --force --deep --options runtime --timestamp \
    --sign "$SIGN_IDENTITY" "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
  spctl --assess --type execute --verbose=2 "$APP"
else
  echo "SIGN_IDENTITY not set: ad-hoc signing (not distributable)"
  codesign --force --deep --sign - "$APP"
  codesign --verify --deep --strict --verbose=2 "$APP"
fi

hdiutil create -volname "ScriptCraft $VERSION" -srcfolder "$STAGE" \
  -ov -format UDZO "$DMG"
shasum -a 256 "$DMG" | tee "$DMG.sha256"

if [ -n "${NOTARY_PROFILE:-}" ]; then
  if [ -z "${SIGN_IDENTITY:-}" ]; then
    echo "NOTARY_PROFILE requires SIGN_IDENTITY" >&2
    exit 1
  fi
  echo "Submitting to notarytool (profile: $NOTARY_PROFILE)"
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
  spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG"
  echo "Notarised and stapled: $DMG"
else
  echo "NOTARY_PROFILE not set: DMG is not notarised"
fi
