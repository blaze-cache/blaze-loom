# Method parity matrix

**Version:** `0.2.0` (skeleton)
**Date:** 2026-11-24
**Relates to:** [Roadmap to adoption](plan/ADOPTION-ROADMAP.md) (Phase 3 / G3), [v0.2.0 plan](plan/0.2.0.md)

Tracks every distinct wasm method `packages/core/src/cache.ts` (monorepo) calls against its
flat-ABI lowering in this crate. A drop-in engine must cover the whole surface, not just
`get`/`set` — this file makes that surface visible before anyone estimates Phase 1–4.

**Status:** skeleton. Rows are filled in Phase 3 (0.4.0), when the inventory is ported and each
lowering lands. 0.2.0 implements only the core row group below.

## Core (0.2.0)

| Method | Flat-ABI lowering | Schema entry | Test source | Status |
|---|---|---|---|---|
| `get_sync(key)` | `__blaze_cache_get_sync` | `Cache.get_sync` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |
| `set(key, value, { ttl, tags })` | `__blaze_cache_set` | `Cache.set` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |
| `has(key)` | `__blaze_cache_has` | `Cache.has` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |
| `delete(key)` | `__blaze_cache_delete` | `Cache.delete` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |
| `stats()` | `__blaze_cache_stats` | `Cache.stats` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |
| `open(config)` / `close()` | `__blaze_cache_open` / `__blaze_cache_close` | `Cache.open` / `Cache.close` | `tests/abi.rs`, `js/harness.mjs` | planned 0.2.0 |

## Extended (0.4.0)

Ids, tags/query, bytes scratch, drop log, batch operations, and the remaining ~50 methods used by
`packages/core/src/cache.ts`. **Not started** — one row per method, added when the Phase 3
inventory lands: `method | flat-ABI lowering | schema entry | test source | status`.
