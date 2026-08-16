#!/usr/bin/env zsh

# TODO: discover version from environment / build / tag?

# intel
rustup target add x86_64-apple-darwin
# AS
rustup target add aarch64-apple-darwin

# local pinning 
source .env

# unwieldy, maybe we can do this better with cargo metadata and jq
VERSION=$(cargo metadata --format-version=1 --no-deps | jq '.packages[3].version' --raw-output)

cargo build -p brhap-app --target aarch64-apple-darwin
cargo build -p brhap-app --target x86_64-apple-darwin

mkdir -p target/universal-apple-darwin/release/bundle/BRhap.app/Contents/MacOS
mkdir -p target/universal-apple-darwin/release/bundle/BRhap.app/Contents/Resources

lipo -create \
  target/aarch64-apple-darwin/release/brhap-app \
  target/x86_64-apple-darwin/release/brhap-app \
  -output target/universal-apple-darwin/release/bundle/BRhap.app/Contents/MacOS/brhap-app

file target/universal-apple-darwin/release/bundle/BRhap.app/Contents/MacOS/brhap-app # check that it is universal

cp -rp ../app-rs/src-tauri/icons/icons.icns target/universal-apple-darwin/release/bundle/BRhap.app/Contents/Resources/icons.icns

codesign --sign "$APPLE_IDENTITY" --timestamp --options runtime --deep --force target/universal-apple-darwin/release/bundle/BRhap.app/Contents/MacOS/brhap-app

# TODO: brew install create-dmg
create-dmg \
    --volname "BRhap ${VERSION}" \
    --volicon "target/universal-apple-darwin/release/bundle/BRhap.app/Contents/Resources/icons.icns" \
    --window-pos 200 120 \
    --window-size 600 400 \
    --icon-size 100 \
    --icon "BRhap.app" 175 120 \
    --app-drop-link 425 120 \
    "target/universal-apple-darwin/release/bundle/dmg/brhap_${VERSION}_universal.dmg" \
    "target/universal-apple-darwin/release/bundle/BRhap.app"

codesign --sign "$APPLE_IDENTITY" --timestamp --options runtime --deep --force target/universal-apple-darwin/release/bundle/dmg/brhap_${VERSION}_universal.dmg

xcrun notarytool submit "target/universal-apple-darwin/release/bundle/dmg/brhap_${VERSION}_universal.dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
xcrun stapler staple "target/universal-apple-darwin/release/bundle/dmg/brhap_${VERSION}_universal.dmg"

# assess (uncomment to debug)
spctl --assess --type open --context context:primary-signature -vv "target/universal-apple-darwin/release/bundle/dmg/brhap_${VERSION}_universal.dmg"
