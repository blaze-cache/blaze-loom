# blaze-cache-wasm

An **independent** Rust project that will become Blaze Cache's future WebAssembly binding layer.

It is deliberately separate from the current monorepo crate (`blaze-cache/crates/blaze-cache-wasm`)
and does not depend on the `wasm-bindgen` glue used there. The binding ABI is a 0.1.0 design
decision, not a foregone conclusion — see [`docs/plan/0.1.0.md`](docs/plan/0.1.0.md).

## Status

`0.1.0` — foundation only. The crate compiles for the host and `wasm32-unknown-unknown`, denies
unsafe code (with one documented export-attribute exception), exports `ABI_VERSION = 0` through
`__blaze_abi_version()`, and carries the project plan, ADR and ABI schema. No cache API is
exported yet.

## Layout

```text
Cargo.toml                       crate manifest (no binding-ABI dependency selected yet)
rust-toolchain.toml              stable + wasm32-unknown-unknown + rustfmt/clippy
src/lib.rs                       crate root, safety policy, ABI_VERSION + flat export
js/abi-check.mjs                 JS-side ABI check against the raw wasm module
docs/README.md                   documentation index
docs/plan/0.1.0.md                v0.1.0 project plan
docs/decisions/0001-abi-strategy.md  ADR: flat raw ABI + schema generator
docs/decisions/blaze.loom         the ABI contract as a schema
```

## Build & test

```sh
cargo test                                   # host
cargo build --target wasm32-unknown-unknown   # Wasm target
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
node js/abi-check.mjs            # checks __blaze_abi_version() == 0
```

The `wasm32-unknown-unknown` target and the components above are declared in
`rust-toolchain.toml`, so rustup installs them automatically on first use.

## Git hooks

The repository ships dependency-free Git hooks in `.githooks/`:

* `commit-msg` — Conventional Commits check (`scripts/check-commit-msg.sh`), no npm needed.
* `pre-push` — `cargo fmt --check`, clippy `-D warnings`, `cargo test`, the wasm target build
  and `node js/abi-check.mjs`. Every branch is gated the same way.

To enable them in a clone, run `git config core.hooksPath .githooks` once.

## Community

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before
opening issues or pull requests.
