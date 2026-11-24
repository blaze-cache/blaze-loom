#!/usr/bin/env sh
# Dependency-free verification gate to run before publishing to crates.io.
# Mirrors .githooks/pre-push, then checks the release profile and the packaged crate itself.
set -eu

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --target wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
node js/abi-check.mjs

cargo package --allow-dirty --list
cargo publish --dry-run --allow-dirty

printf 'Pre-publish checks passed. No upload was performed.\n'
