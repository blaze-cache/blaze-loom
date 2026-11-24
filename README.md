# blaze-cache-wasm

An **independent** Rust project that will become Blaze Cache's future WebAssembly binding layer.

It is deliberately separate from the current monorepo crate (`blaze-cache/crates/blaze-cache-wasm`)
and does not depend on the `wasm-bindgen` glue used there. The binding ABI is a 0.1.0 design
decision, not a foregone conclusion — see [`docs/plan/0.1.0.md`](docs/plan/0.1.0.md). The full
path from this foundation to a published, drop-in engine for our projects is in
[`docs/plan/ADOPTION-ROADMAP.md`](docs/plan/ADOPTION-ROADMAP.md).

## Status

`0.1.0` — foundation only. The crate compiles for the host and `wasm32-unknown-unknown`, denies
unsafe code (with one documented export-attribute exception), exports `ABI_VERSION = 0` through
`__blaze_abi_version()`, and carries the project plan, ADR and ABI schema. No cache API is
exported yet. The [M3 ABI spike](docs/decisions/m3-spike.md) is complete and confirmed the flat
raw + Loom choice. The `0.1.0` tag (`0f3da91`) is published and the release gate is green.

## Layout

```text
Cargo.toml                       crate manifest (no binding-ABI dependency selected yet)
rust-toolchain.toml              stable + wasm32-unknown-unknown + rustfmt/clippy
src/lib.rs                       crate root, safety policy, ABI_VERSION + flat export
tests/abi.rs                     external-consumer integration tests for the public surface
js/abi-check.mjs                 JS-side ABI check against the raw wasm module
scripts/pre-publish.sh            full verification gate before a crates.io release
.github/workflows/ci.yml          runs the same gate on every push and pull request
docs/README.md                   documentation index
docs/plan/0.1.0.md                v0.1.0 project plan
docs/decisions/0001-abi-strategy.md  ADR: flat raw ABI + schema generator
docs/decisions/blaze.loom         the ABI contract as a schema
docs/decisions/m3-spike.md        four-arm ABI benchmark results and decision
spike/m3/                          M3 arms, harness and reproducible build script
```

## Build & test

```sh
cargo test                                   # host
cargo build --target wasm32-unknown-unknown   # Wasm target
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
node js/abi-check.mjs            # checks __blaze_abi_version() == 0
sh scripts/pre-publish.sh        # everything above, plus packaging checks
```

The `wasm32-unknown-unknown` target and the components above are declared in
`rust-toolchain.toml`, so rustup installs them automatically on first use. The integration tests
in `tests/abi.rs` link against the crate as an external consumer would, so the published surface is
checked independently of the crate's own unit tests.

## Before publishing to crates.io

Run `sh scripts/pre-publish.sh`. It runs the same checks as the pre-push hook, builds the
`wasm32-unknown-unknown` target in both debug and release, runs the JS ABI check, then packages the
crate and does a `cargo publish --dry-run`. The dry run verifies the `.crate` contents and builds
them in a clean directory; it does **not** upload anything.

CI runs the same script on every push and pull request, so a red build cannot be published.

## Git hooks

The repository ships dependency-free Git hooks in `.githooks/`:

* `commit-msg` — Conventional Commits check (`scripts/check-commit-msg.sh`), no npm needed.
* `pre-push` — `cargo fmt --check`, clippy `-D warnings`, `cargo test`, the wasm target build
  and `node js/abi-check.mjs`. Every branch is gated the same way.

To enable them in a clone, run `git config core.hooksPath .githooks` once.

## Community

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before
opening issues or pull requests.
