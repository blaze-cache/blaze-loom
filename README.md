# blaze-cache-wasm

An **independent** Rust project that will become Blaze Cache's future WebAssembly binding layer.

It is deliberately separate from the current monorepo crate (`blaze-cache/crates/blaze-cache-wasm`)
and does not depend on the `wasm-bindgen` glue used there. The binding is a flat raw ABI
(`loom-abi-v2`, `ABI_VERSION = 1`) with no generated glue — see
[`docs/decisions/0004-abi-v2.md`](docs/decisions/0004-abi-v2.md) and
[`docs/plan/0.2.0.md`](docs/plan/0.2.0.md). The full path from this foundation to a
published, drop-in engine for our projects is in
[`docs/plan/ADOPTION-ROADMAP.md`](docs/plan/ADOPTION-ROADMAP.md).

## Status

`0.2.0` — the first release with a working cache behind the flat ABI. The crate vendors a
pinned `blaze-cache-core` snapshot (kept in sync by `scripts/sync-core.sh`), exports the full
`loom-abi-v2` surface — `open`, `get_sync`, `set`, `has`, `delete`, `stats`, `close`,
`generation`, `tick` and the arena allocator — with `ABI_VERSION = 1`, and round-trips a `Ref`
handle and raw bytes through `js/harness.mjs` with **no wasm-bindgen and no unsafe code** (one
documented `#[no_mangle]` exception class). Hostile-pointer property tests live in
`tests/abi.rs`. The `0.1.0` foundation tag (`0f3da91`) and the `0.2.0` tag (`605cab1`) are
published; the `0.2.0` release record is in
[`docs/plan/0.2.0.md`](docs/plan/0.2.0.md#release-record).

## Layout

```text
Cargo.toml                       crate manifest (no wasm-bindgen; flat ABI only)
rust-toolchain.toml              stable + wasm32-unknown-unknown + rustfmt/clippy
src/lib.rs                       crate root, safety policy, ABI_VERSION
src/abi.rs                       every #[no_mangle] export + the shared ErrorCode enum
src/arena.rs                     fixed-capacity scratch arena (bounds-checked host pointers)
src/cache.rs                     ids, slots, generations — the registry behind the exports
src/clock.rs                     host-driven monotonic clock (__blaze_tick; ADR-0002)
src/engine/                      vendored blaze-cache-core snapshot (ADR-0002)
tests/abi.rs                     external-consumer property tests for the flat surface
js/abi-check.mjs                 contract check: export set, version, error codes, generation
js/harness.mjs                   end-to-end round-trip against the release module
scripts/sync-core.sh             re-vendor the pinned engine snapshot
scripts/pre-publish.sh            full verification gate before a crates.io release
.github/workflows/ci.yml          runs the same gate on every push and pull request
docs/README.md                   documentation index
docs/plan/0.1.0.md                v0.1.0 project plan
docs/plan/0.2.0.md                v0.2.0 project plan (current release)
docs/decisions/0001-abi-strategy.md  ADR: flat raw ABI + schema generator
docs/decisions/blaze.loom         the frozen loom-abi-v1 contract
docs/decisions/blaze-v2.loom      the loom-abi-v2 contract (0.2.0)
docs/decisions/m3-spike.md        four-arm ABI benchmark results and decision
spike/m3/                          M3 arms, harness and reproducible build script
```

## Build & test

```sh
cargo test --locked                         # host tests (engine + ABI property tests)
cargo build --release --target wasm32-unknown-unknown
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
node js/abi-check.mjs            # contract: export set, ABI_VERSION == 1, error codes
node js/harness.mjs              # end-to-end: Ref + bytes round-trip through the release wasm
sh scripts/pre-publish.sh        # everything above (with wasm-opt -Oz), plus packaging checks
```

The `wasm32-unknown-unknown` target and the components above are declared in
`rust-toolchain.toml`, so rustup installs them automatically on first use. `wasm-opt`
(binaryen) is needed for the release gate's `-Oz` step — `brew install binaryen`. The
integration tests in `tests/abi.rs` link against the crate as an external consumer would, so
the published surface is checked independently of the crate's own unit tests.

## Before publishing to crates.io

Run `sh scripts/pre-publish.sh`. It runs the same checks as the pre-push hook, builds the
`wasm32-unknown-unknown` target in both profiles, optimizes the release module with
`wasm-opt -Oz`, runs the JS contract check and the end-to-end harness against that exact
artifact, verifies `cargo tree` shows no monorepo crate dependency, then packages the crate
and does a `cargo publish --dry-run`. The dry run verifies the `.crate` contents and builds
them in a clean directory; it does **not** upload anything.

CI runs the same script on every push and pull request, so a red build cannot be published.

## Git hooks

The repository ships dependency-free Git hooks in `.githooks/`:

* `commit-msg` — Conventional Commits check (`scripts/check-commit-msg.sh`), no npm needed.
* `pre-push` — `cargo fmt --check`, clippy `-D warnings`, `cargo test --locked`, the release
  wasm build and `node js/abi-check.mjs`. Every branch is gated the same way.

To enable them in a clone, run `git config core.hooksPath .githooks` once.

## Community

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before
opening issues or pull requests.
