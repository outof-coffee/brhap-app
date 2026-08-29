#!/usr/bin/env zsh
set -euo pipefail

# local pinning
source .env

# scoped to brhap-app specifically: this is a workspace with four members,
# so .packages[0] from `cargo metadata` is not reliably this crate
VERSION=$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "brhap-app") | .version')

# intel
rustup target add x86_64-apple-darwin
# AS
rustup target add aarch64-apple-darwin

cargo build --release -p brhap-app --target x86_64-apple-darwin
cargo build --release -p brhap-app --target aarch64-apple-darwin

# universal binary for the packager step to consume.
# the crate/binary is named brhap-app; the product name is brhap.
UNIVERSAL_DIR="target/universal-apple-darwin/release"
mkdir -p "$UNIVERSAL_DIR"

# Clear last run's bundles. Without this a rename or a failed package step
# leaves a stale dmg here and the notarize step below submits that instead.
rm -rf "$UNIVERSAL_DIR"/*.app "$UNIVERSAL_DIR"/*.dmg

lipo -create \
  target/x86_64-apple-darwin/release/brhap-app \
  target/aarch64-apple-darwin/release/brhap-app \
  -output "$UNIVERSAL_DIR/brhap-app"

# The signing identity is a credential, so it never lives in a tracked file.
# cargo-packager only reads it from config, not from the environment, so the
# config is generated here from .env and is gitignored.
cat > packager.toml <<EOF
# auto-generated - do not commit or edit!
# name is set so cargo-packager skips its own auto-detect, which chdirs
# into the config file path rather than its parent and errors out.
name = "brhap-app"
product-name = "brhap"
version = "$VERSION"
identifier = "coffee.outof.brhap"
icons = ["brhap-app/assets/icon.icns"]
out-dir = "$UNIVERSAL_DIR"

[[binaries]]
path = "brhap-app"
main = true

[macos]
minimum-system-version = "10.13"
signing-identity = "$APPLE_SIGNING_IDENTITY"
EOF

cargo packager -f app,dmg -c packager.toml --target universal-apple-darwin

DMG="target/universal-apple-darwin/release/brhap_${VERSION}_universal.dmg"

# notarize dmg
xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait

# staple dmg
xcrun stapler staple "$DMG"

# assess (uncomment to debug)
# spctl --assess --type open --context context:primary-signature -vv "$DMG"
