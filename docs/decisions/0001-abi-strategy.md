# ADR-0001: Wasm ↔ JS ABI strategy

**Status:** proposed; provisionally accepted for 0.1.0, to be revisited after M3 (M3 may supersede this decision)
**Date:** 2026-11-22
**Deciders:** blaze-cache maintainers
**Relates to:** [v0.1.0 plan](../plan/0.1.0.md), [blaze.loom schema](blaze.loom)

## Context

The existing Blaze Cache bindings live in `blaze-cache/crates/blaze-cache-wasm` and are generated
with `wasm-bindgen`. This project exists to evaluate and eventually own the future binding layer
without inheriting that coupling. The 0.1.0 plan sets hard constraints:

* no dependency on the current binding crate, `wasm-bindgen`, `js-sys`, `serde-wasm-bindgen` or
  `wasm-bindgen-test`;
* no unsafe blocks or calls (one export-attribute lint exception, see below);
* a public, test-pinned `ABI_VERSION`;
* no public cache API in 0.1.0 — the ABI itself is the deliverable.

The central decision is how JS and Rust exchange data.

## Options evaluated

1. **Generated glue (a newer `wasm-bindgen` line).** Mature tooling and familiar TypeScript
   types, but reintroduces the generator coupling this project exists to avoid.
2. **Flat raw ABI** — `(ptr, len)` exports plus `ABI_VERSION`, with a hand-written JS decoder.
   Small binary, explicit validation, no Rust dependency.
3. **WebAssembly Component Model / WIT.** Typed, language-neutral contracts, but uneven browser
   runtime support and fast-moving tooling in 0.1.0's timeframe.

Loom sits between (2) and (3): a flat raw ABI with a schema-driven build-time generator. It adds
**no Rust dependency** — the generator is a build tool, not linked code. The practical choice is
therefore option 2.5: **flat raw ABI, schema-specified**.

Loom is still a generator, so version skew is not eliminated. It is reduced because the generator
runs at build time, its output is plain JS/TS with no runtime glue, and the schema, not the tool,
is the ABI contract. M3 includes Loom as its own arm.

## Decision

Adopt a **flat raw ABI** for `blaze-cache-wasm`, specified by the [`blaze.loom`](blaze.loom)
schema. Loom-style generation is a build-time tool only; the crate keeps zero binding
dependencies. `ABI_VERSION = 0` corresponds to `loom-abi-v1`. The only 0.1.0 export is
`__blaze_abi_version()`, checked by [`js/abi-check.mjs`](../../js/abi-check.mjs) before any other
call; a mismatch throws rather than silently misdecoding.

The schema is the decision artifact: it fixes, for later implementation, that keys are opaque
buffers, values are views with a documented staleness rule, `get_sync` and `get` are distinct
operations, and `Cache` is linear with an explicit `close`. No TS package is shipped in 0.1.0.

### Host-input strategy (Approach B: copy-in, no raw slices)

A raw ABI normally needs `slice::from_raw_parts`, which is unsafe. To honour the safety goal the
host copies key bytes into a Wasm-owned arena; Rust reads from a safe `Vec<u8>` the arena owns.
No host pointer is ever turned into a Rust slice. The cost is one memcpy per call (expected to be
small; to be measured in M3); the 1 KB-value case will also be measured in M3 before any
optimisation is considered.

Copy-in requires an `alloc`-style export so the host knows where to write. That export is added
with the first key API (0.2.0). Rust owns bounds checks on length; the host never supplies a length
Rust trusts without validation. Oversized or reentrant writes return an error code rather than
growing the arena unchecked.

An audited `#[trusted]` unsafe boundary is explicitly deferred to 0.2.0; if profiling justifies
it, that would be a single documented site, not a blanket relaxation.

### Rust lint exception

Rust's current `unsafe_code` lint treats the `#[no_mangle]` attribute required by a flat export
as unsafe code. The crate therefore uses `#![deny(unsafe_code)]` with one explicitly allowed,
documented exception on `__blaze_abi_version()`. There are no `unsafe` blocks and no unsafe
calls; the exception is an export attribute only.

## Consequences

* **Now (0.1.0):** no generated glue, no unsafe blocks, one export attribute exception, loud
  version check.
* **Contract for 0.2.0 (not implemented):** JS-side view validation and generation-based
  stale-view rejection.
* **Portability:** a flat `(ptr, len)` ABI is expected to work on V8, JSC and SpiderMonkey; to be
  verified in M3. Older engines need no extra build profile at this stage because no typed-binding
  runtime is used.
* **Binary size / cold start:** no glue code linked; 0.1.0's raw module exports only `memory`
  and `__blaze_abi_version`.
* **DX:** TypeScript bindings are generated from `blaze.loom` in a later version, not hand-
  maintained here. 0.1.0 ships the schema, not a package.
* **Version skew:** `ABI_VERSION` is checked at instantiation; failure is loud.

## View-detachment rule (to be enforced when values ship)

A `(ptr, len)` returned by `get_sync` is valid only until the next cache mutation or a
`memory.grow`. The JS runtime must re-create views when `memory.buffer` changes and invalidate
views when the cache generation advances. The schema's `view<buffer>` alias documents this; the
M3 spike must include the check. No value API exists in 0.1.0, so this is a contract for 0.2.0.

## Compliance with 0.1.0 acceptance criteria

* [x] standalone crate, no binding-generator dependency
* [x] no unsafe blocks; one documented export-attribute exception
* [x] `ABI_VERSION` public, exported, and pinned by a Rust test plus a JS check
* [x] `blaze.loom` is the ABI decision artifact
* [ ] M3 spike evidence — pending: four arms, binary size, cold start, call cost, 1 KB set cost
