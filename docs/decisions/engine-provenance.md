# Engine provenance — vendored `blaze-cache-core`

**Version:** `0.2.0`
**Date:** 2026-11-24
**Relates to:** [ADR-0002](0002-engine-source.md) (D1: vendor decision), [v0.2.0 plan](../plan/0.2.0.md)

This file records exactly where the vendored engine came from and how to refresh it. The
vendored tree is **byte-faithful upstream source** with manifest inlining only — no behavioural
edits (ADR-0002).

## Pinned upstream

| Field | Value |
|---|---|
| Repository | `../blaze-cache` (https://github.com/blaze-cache/blaze-cache) |
| Branch | `v0.5.0-beta.1` (recorded, **not tracked** — a pin, not a branch follow) |
| Commit | `bf0bf2781e2f4ae34f0eab6b9776f79bc76fa8d1` |
| Tree state at pin | clean (`git status --porcelain` empty) |
| Upstream crate | `crates/blaze-cache-core` |
| Upstream version | `0.5.0-beta.1` |

## Inlined manifest values

Upstream inherits these from `[workspace.package]`; they are inlined here because this repo is a
standalone crate, not that workspace:

| Field | Upstream source | Inlined value |
|---|---|---|
| `version` | `workspace.package.version` | `0.5.0-beta.1` (engine identity; the published crate version stays `0.2.0`) |
| `edition` | `workspace.package.edition` | `2024` upstream — **vendored source compiles under this crate's edition 2021**; see §Edition note |
| `license` | `workspace.package.license` | `MIT` (same as this crate) |
| `repository` | `workspace.package.repository` | `https://github.com/blaze-cache/blaze-cache` (recorded here; this crate's `repository` field is unchanged) |
| `rust-version` | `workspace.package.rust-version` | `1.85` (same as this crate) |

Dependency versions are pinned to upstream's `Cargo.lock` resolutions. **`web-time` is the
one dependency dropped** — see ADR-0002's owned deviation: its wasm backend is wasm-bindgen
glue, so the engine's `use web_time::Instant;` lines are rewritten to `use crate::clock::Instant;`
by the sync script and [`src/clock.rs`](../../src/clock.rs) supplies the type (native:
`std::time::Instant`, which is what `web_time::Instant` re-exports there; wasm: host-ticked
via `__blaze_tick`).

| Dependency | Manifest req | Pinned at sync | Features |
|---|---|---|---|
| `foldhash` | `0.2` | `0.2.0` | default |
| `serde` | `1` | `1.0.229` | `derive` |
| `smallvec` | `1` | `1.16.5` | `serde` |
| ~~`web-time`~~ | ~~`1.1`~~ | **removed** (wasm path is wasm-bindgen glue — ADR-0002) | — |

## Edition note

Upstream is edition 2024; this crate is edition 2021 (kept, so `#[no_mangle]` stays spelled as
the 0.1.0 documented exception). The vendored source was checked for 2024-only constructs before
landing: no `gen` identifiers, no `unsafe` anywhere (`#![forbid(unsafe_code)]` upstream), and
the single RPIT (`Cache::keys`) has its lifetime in bounds, which both editions accept. If a
future sync pulls in edition-2024-only syntax, the sync fails loudly at `cargo build` and the
choice is recorded here (either a mechanical edition shim in the sync script or a crate edition
bump — the latter changes the `#[no_mangle]` spelling and needs an ADR).

## Layout

```
src/clock.rs     ← crate-owned: the wasm clock shim (NOT upstream — ADR-0002's deviation)
src/engine/
  mod.rs        ← upstream src/lib.rs (module header, docs, re-exports)
  bytes.rs      ← upstream src/bytes.rs
  cache.rs      ← upstream src/cache.rs (+ cache/tests.rs, cache/stress.rs, cache/query_tests.rs)
  entry.rs      ← upstream src/entry.rs
  eviction.rs   ← upstream src/eviction.rs (+ eviction/tests.rs)
  node.rs       ← upstream src/node.rs
  query.rs      ← upstream src/query.rs
  stats.rs      ← upstream src/stats.rs
  tags.rs       ← upstream src/tags.rs
tests/engine_allocations.rs  ← upstream tests/allocations.rs (crate path rewritten)
```

Upstream `src/lib.rs` becomes `src/engine/mod.rs`; its `mod` declarations and doc examples keep
working because the module tree mirrors upstream. The integration test's `use blaze_cache_core::…`
is rewritten to `use blaze_cache_wasm::engine::…` — the only content change, recorded by the
sync script. Upstream `tests/allocations.rs` lives here as `tests/engine_allocations.rs` so it
runs in this crate's `cargo test`.

## Sync procedure

```sh
sh scripts/sync-core.sh [path/to/blaze-cache]
```

1. Resolves the checkout (default `../blaze-cache`).
2. **Verifies `HEAD == bf0bf278…` and the tree is clean** — refuses to sync from any other
   commit unless `--allow-dirty` is passed (then it prints a warning and records nothing).
3. Copies `src/**.rs` into `src/engine/` (lib.rs → mod.rs) and `tests/allocations.rs` to
   `tests/engine_allocations.rs`.
4. Rewrites `use blaze_cache_core::` → `use blaze_cache_wasm::engine::` in the integration test.
5. Runs `cargo fmt --all`, because upstream's style is not this repo's and the gate runs
   `cargo fmt --check` — the script's fixed point must be gate-clean (copy → rewrite → fmt
   is idempotent: rustfmt normalizes its own output on the next run).
6. Prints a diffstat so the operator sees exactly what moved.

To move the pin: land the fix upstream, update the commit in **both** this file and
`scripts/sync-core.sh`, run the sync, run `sh scripts/pre-publish.sh`, and commit all three
together. The pinned commit exists in exactly two places and they must agree — the script
asserts it.

## Verification at this pin

* `cargo test` — engine unit tests (cache, eviction, query, tags) plus this crate's ABI tests.
* `cargo build --target wasm32-unknown-unknown` and `--release` — both targets build.
* `cargo tree` — no `../blaze-cache/crates/*` path dependency; the four pinned deps only.
* `sh scripts/pre-publish.sh` — full gate green.
