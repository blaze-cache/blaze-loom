#!/usr/bin/env sh
# Build all four M3 arms with the same release profile and wasm-opt -Oz pass.
# wasm-pack applies wasm-opt automatically; the other arms are normalized here so binary
# sizes are comparable. Run: sh spike/m3/build.sh
set -eu
cd "$(dirname "$0")"

WOPT="./node_modules/.bin/wasm-opt"
JCO="./node_modules/.bin/jco"

cargo build --release --target wasm32-unknown-unknown --manifest-path flat-raw/Cargo.toml
cargo build --release --target wasm32-unknown-unknown --manifest-path loom/Cargo.toml
cargo build --release --target wasm32-unknown-unknown --manifest-path component/Cargo.toml

for f in \
  flat-raw/target/wasm32-unknown-unknown/release/spike_flat_raw.wasm \
  loom/target/wasm32-unknown-unknown/release/spike_loom.wasm \
  component/target/wasm32-unknown-unknown/release/spike_component.wasm
do
  "$WOPT" -Oz "$f" -o "$f.opt"
  mv "$f.opt" "$f"
done

node loom/generate.mjs

(cd wasm-bindgen && wasm-pack build --release --target web --out-dir pkg)

wasm-tools component new \
  component/target/wasm32-unknown-unknown/release/spike_component.wasm \
  -o component/component.wasm
"$JCO" transpile component/component.wasm -o component/out --name spike

printf 'All four arms built.\n'
