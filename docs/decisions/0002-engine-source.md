# ADR-0002: Engine source (D1)

**Status:** accepted for 0.2.0
**Date:** 2026-11-24
**Deciders:** blaze-cache maintainers
**Relates to:** [v0.2.0 plan](../plan/0.2.0.md), [engine provenance](engine-provenance.md), [ADR-0004](0004-abi-v2.md)

## Context

0.2.0 must answer `get`/`set`/`has`/`delete`/`stats` through the flat ABI. The cache engine
(keys, TTL, SIEVE, tags) already exists as `blaze-cache-core` in the monorepo — the roadmap's
first hard rule is **never reimplement**. Two ways to consume it:

1. **Extract** `blaze-cache-core` into its own published crate that both repos depend on.
2. **Vendor** a pinned snapshot of the crate into this repository with a sync script.

Extraction is the right end state: one source of truth, versioned releases, both repos bump a
dependency. But it needs crates.io publishing rights, workspace surgery in the monorepo, and a
release cadence the monorepo (currently `v0.5.0-beta.1`) has not committed to. It is also a
Phase-4-adjacent change to the monorepo, which the roadmap gates behind `0.5.0-rc.1`.

Vendoring is mechanically simple and unblocks 0.2.0 immediately, at the cost of a snapshot that
can drift from upstream.

## Decision

**Vendor a pinned snapshot** of `blaze-cache-core` into this repository at the commit recorded in
[engine-provenance.md](engine-provenance.md), with:

* `scripts/sync-core.sh` — re-copies the snapshot from a given `../blaze-cache` checkout and
  fails if the working tree it copies from does not match the pinned commit (unless
  `--allow-dirty`), so a sync is always a deliberate, recorded act;
* inlined manifest values (the monorepo's `workspace = true` fields become concrete values —
  see provenance for the exact inlined set);
* the upstream `#![forbid(unsafe_code)]` preserved — the vendored engine must satisfy this
  crate's safety policy without edits.

Extraction (option a) remains the target: when `blaze-cache-core` is published, this repo
replaces the vendored tree with a version-pinned dependency. That swap is a dependency bump,
not a rewrite, because the vendored tree is byte-faithful upstream source.

**Never reimplement.** The vendored tree is upstream code with manifest inlining only; no
behavioural edits. If a fix is needed, it goes upstream first and is picked up by a sync.

### Owned deviation: the wasm clock (web-time removal)

One exception to byte-faithfulness, because keeping `web-time` would break the ABI thesis:

* `web-time` 1.1.0's `wasm32-unknown-unknown` backend is built on **`wasm-bindgen` +
  `js-sys`** (non-optional target dependencies) — thousands of `__wbindgen_*` imports, a
  ~7.9 MB module, and `WebAssembly.instantiate(bytes, {})` fails outright. ADR-0001 rules
  that glue out.
* `std::time::Instant::now()` **traps** on `wasm32-unknown-unknown` (verified: the call
  compiles to `unreachable`), so it is not a drop-in replacement either.

Resolution: the engine's `use web_time::Instant;` lines are mechanically rewritten to
`use crate::clock::Instant;`, and [`src/clock.rs`](../../src/clock.rs) supplies the type:

* **off-wasm:** a re-export of `std::time::Instant` — which is exactly what
  `web_time::Instant` is there, so native behaviour is unchanged;
* **on wasm:** a `u64`-nanos instant read from a host-ticked cell. The host calls the new
  flat export `__blaze_tick(now_ms)` before each ABI call (monotonic `performance.now()` or
  `Date.now()` both work); time is frozen — not broken — until it does.

The rewrite and the handful of comment amendments that describe it are owned by
`scripts/sync-core.sh` and recorded in [engine-provenance.md](engine-provenance.md). This is
the **only** deviation from byte-faithful vendoring; any future one needs an amendment here.

## Consequences

* `cargo tree` shows no `../blaze-cache/crates/*` dependency — the 0.2.0 acceptance gate.
* The snapshot can drift; mitigated by the pinned commit in provenance and the sync script's
  commit check.
* Upstream fixes require a sync + commit here; acceptable at Phase 1, one of the reasons
  extraction (a) is the eventual target.
* `cargo package` includes the vendored tree (it is inside `src/` or `engine/`, not `spike/`),
  so the published crate is self-contained.
