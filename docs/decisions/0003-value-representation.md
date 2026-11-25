# ADR-0003: Value representation (D2)

**Status:** accepted for 0.2.0
**Date:** 2026-11-24
**Deciders:** blaze-cache maintainers
**Relates to:** [v0.2.0 plan](../plan/0.2.0.md), [ADR-0001](0001-abi-strategy.md) (view rule), [`blaze.loom`](blaze.loom) → `blaze-v2.loom`

## Context

Values cross the Wasm boundary in two fundamentally different ways:

1. **Bytes only** — everything is a `buffer`; JS objects must be serialised into bytes before
   `set` and parsed back after `get`. Simple ABI, but it destroys object identity, costs a
   serialisation on every boundary crossing, and cannot round-trip a live `JsValue`.
2. **Integer handles** — wasm stores an opaque `u32`; the real object lives in a JS-side
   `Map<id, value>`. Identity is preserved (the same object comes back), no serialisation, and
   wasm never touches the object. The cost is host-side bookkeeping: ids must be allocated and
   eventually freed.
3. **`externref` table** — wasm stores JS objects directly as `externref`. No ids, no maps, but
   it forces an externref-capable ABI (not a flat raw one) and hands GC visibility into wasm.

The legacy crate proves option 2 works in production: `Value::{Ref(JsValue), Bytes(BytesRef)}`
(`crates/blaze-cache-wasm/src/lib.rs:53`) with a JS-authoritative `Map<id, V>`.

## Decision

**Option (b): `alias Ref = handle<u32>` + a JS-owned `Map`, with `view<buffer>` kept for binary
mode.**

* `set` has two kinds: `0 = Ref` (wasm stores the `u32` handle; `value_len` ignored) and
  `1 = Bytes` (wasm stores a copy of the bytes).
* Wasm treats handles as **opaque**: it never dereferences, guesses or recycles them. Handle
  allocation, identity and freeing are the host's responsibility.
* **Lifetime:** in 0.2.0 (wasm-side only) the harness owns the `Map` and disposes it explicitly
  at `close()`. The `FinalizationRegistry` that makes leak-freedom automatic is 0.3.0 work (D4
  and the roadmap's Phase 2); the 0.3.0 browser leak test is the enforcement point.
* Binary mode keeps ADR-0001's view rule: a returned `(ptr, len)` is valid until the next
  mutation or `memory.grow`, enforced by the generation counter (0.2.0) and the runtime check
  (0.3.0).

## Consequences

* Object identity round-trips: JS gets back the exact object it stored, no JSON tax.
* Wasm never holds a JS pointer — the flat ABI stays `externref`-free (ADR-0001).
* Handle leaks are possible until 0.3.0 ships `FinalizationRegistry`; 0.2.0 documents this and
  disposes explicitly in the harness.
* A `Ref` value's bytes are meaningless to wasm — `get_sync` on a `Ref` key returns the handle
  as a 4-byte little-endian `u32` payload, and only the host's `Map` gives it meaning.
