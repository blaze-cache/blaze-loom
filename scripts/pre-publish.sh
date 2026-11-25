#!/usr/bin/env sh
# Dependency-free verification gate to run before publishing to crates.io.
# Mirrors .githooks/pre-push, then checks the release profile and the packaged crate itself.
set -eu

# The optimized release module is the artifact that ships, so binaryen is a gate
# prerequisite (CI installs it via apt; locally: brew install binaryen).
command -v wasm-opt >/dev/null 2>&1 || {
  printf 'pre-publish: wasm-opt not found — install binaryen (brew install binaryen; CI: apt-get install binaryen)\n' >&2
  exit 1
}

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --target wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown

# Optimize the release module in place, via a temp file so the build output is never
# truncated mid-write. Every check below runs against exactly the artifact that ships.
RELEASE=target/wasm32-unknown-unknown/release/blaze_cache_wasm.wasm
wasm-opt -Oz "$RELEASE" -o "$RELEASE.opt"
mv "$RELEASE.opt" "$RELEASE"
printf 'wasm-opt -Oz: %s bytes\n' "$(wc -c < "$RELEASE" | tr -d ' ')"

# Contract + end-to-end checks against the (optimized) release module: no path argument
# means the release build for both scripts.
node js/abi-check.mjs
node js/harness.mjs

# Vendoring must be complete: no path dependency on the monorepo's crates may remain.
TREE=$(cargo tree)
if printf '%s\n' "$TREE" | grep -q 'blaze-cache/crates'; then
  printf 'cargo tree shows a monorepo crate dependency — vendoring is incomplete.\n' >&2
  exit 1
fi

cargo package --allow-dirty --list
cargo publish --dry-run --allow-dirty

printf 'Pre-publish checks passed. No upload was performed.\n'
