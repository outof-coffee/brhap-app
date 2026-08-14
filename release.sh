#!/usr/bin/env zsh

# intel
rustup target add x86_64-apple-darwin
# AS
rustup target add aarch64-apple-darwin

# local pinning 
source .env

pushd app-rs
cargo tauri build --bundles app,dmg --target universal-apple-darwin

