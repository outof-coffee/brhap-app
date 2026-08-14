#!/usr/bin/env zsh

# TODO: discover version from environment / build / tag?

# intel
rustup target add x86_64-apple-darwin
# AS
rustup target add aarch64-apple-darwin

# local pinning 
source .env

pushd app-rs
cargo tauri build --bundles app,dmg --target universal-apple-darwin

# staple the app
xcrun stapler staple "target/universal-apple-darwin/release/bundle/macos/brhap.app"

# notarize dmg
xcrun notarytool submit "target/universal-apple-darwin/release/bundle/dmg/brhap_0.1.0_universal.dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait

# staple dmg 
xcrun stapler staple "target/universal-apple-darwin/release/bundle/dmg/brhap_0.1.0_universal.dmg"

# assess (uncomment to debug)
# spctl --assess --type open --context context:primary-signature -vv "target/universal-apple-darwin/release/bundle/dmg/brhap_0.1.0_universal.dmg"

popd
