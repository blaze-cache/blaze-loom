# blaze-cache-wasm — Roadmap to Adoption

**Version:** `0.1.0+` (planning document, unversioned)
**Date:** 2026-11-24
**Status:** proposed — needs the decisions in §3 before Phase 1 starts
**Repository:** `blaze-cache-wasm` (standalone), target consumer: `../blaze-cache` monorepo
**Relates to:** [v0.1.0 plan](0.1.0.md), [ADR-0001](../decisions/0001-abi-strategy.md),
[blaze.loom schema](../decisions/blaze.loom), [M3 spike report](../decisions/m3-spike.md)

---

## 0. What "we can use this wasm in our projects" means

This plan ends when **all three** statements are true:

1. **Distributable.** This repo publishes an npm package containing the raw `.wasm` plus the
   Loom-generated JS/TS bindings and types. Any of our Vite/Node projects can install it and
   call a typed API without wasm-bindgen glue or a bundler-specific plugin.
2. **Drop-in for existing consumers.** `@blaze-cache/core` in `../blaze-cache` can be built
   against this engine behind a switch (`engine: 'legacy' | 'loom'`) with **no change to its
   public TypeScript API**, and its full test suite plus the examples/benchmarks stay green.
3. **Repeatable and safe.** One build script produces the artifacts, CI runs the same gate
   locally and remotely, ABI skew fails loudly, and a documented rollback returns to the
   current wasm-bindgen engine.

Anything short of that (a crate that compiles but exports no cache API; a package nobody can
install; a parallel API that existing projects cannot call) is **not** adoption.

### Assumptions made in this plan

- The first consumer is the `../blaze-cache` monorepo, currently on `v0.5.0-beta.1` and
  building its engine with `wasm-pack build crates/blaze-cache-wasm` into
  `packages/core/pkg/`, loaded by `packages/core/src/wasm.ts`.
- The 0.1.0 decisions stay in force: flat raw ABI, no binding-generator dependency in Rust,
  `#![deny(unsafe_code)]` with the single export-attribute exception, copy-in host input.
- We do not change the monorepo's public API to adopt this engine. A parallel API in new
  projects is acceptable **only** as a later, separately approved step (Phase 5).
- No dates are committed; phases are ordered by dependency and sized S/M/L.

---

## 1. Current state (evidence)

| Area | This repo (`blaze-cache-wasm`) | Monorepo (`../blaze-cache`) |
|---|---|---|
| Version | `0.1.0` foundation, **not yet tagged** | `0.4.0` package version, `v0.5.0-beta.1` branch |
| Crate | `cdylib` + `rlib`, zero dependencies, `deny(unsafe_code)` | `crates/blaze-cache-wasm` uses `wasm-bindgen`, `js-sys`, `web-time` |
| Exports | `__blaze_abi_version()`, `memory` only | ~55 `#[wasm_bindgen]` methods (get/set/query/tags/ids/drops/bytes/raw/stats) |
| ABI | `ABI_VERSION = 0` (`loom-abi-v1`), checked in Rust + `js/abi-check.mjs` | wasm-bindgen generated `init()` + `wasmUrl`/`module`/`instantiate`/`wasm` loaders in `packages/core/src/wasm.ts` |
| Values | None yet; schema defines `Key = buffer`, `Value = view<buffer>` | `JsValue` externrefs — **live by-reference JS objects**, never cloned |
| Engine | None (`blaze-cache-core` is explicitly a non-goal) | `crates/blaze-cache-core`: SIEVE, TTL, tags, indexes, query, stats, drop log |
| Distribution | None; 0.1.0 explicitly excludes an npm package | `packages/core/pkg/` generated assets, published via `@blaze-cache/core` |
| Verification | `cargo test`, wasm builds, fmt/clippy, `node js/abi-check.mjs`, `scripts/pre-publish.sh`, CI | `pnpm build:wasm && pnpm -r test`, `wasm-pack test --node`, browser + bench suites |

M3 already de-risked the ABI choice (raw + Loom: 18,166 B wasm, ~2.3 KB glue, ~0.22 µs
`get_sync`, ~0.32 µs 1 KB `set`). **M3 measured a `HashMap`, not the cache engine**: TTL,
eviction, tags, views, async, query and a browser matrix are unmeasured. Its `spike.loom`
lowers raw signatures only; lowering the full high-level `blaze.loom` contract is explicitly
future work. That work is this roadmap.

---

## 2. The five gaps to close

| # | Gap | Why it blocks adoption | Severity |
|---|---|---|---|
| G1 | **Engine independence.** The engine lives in `../blaze-cache/crates/blaze-cache-core`; this repo must not depend on it. | Without an engine, no cache API can ship. Copying it silently creates a second source of truth. | Blocker |
| G2 | **Value semantics.** Schema says `Value = view<buffer>` (bytes in wasm memory); existing API stores arbitrary JS objects by reference and promises stable identity (enforced by 0.6.0 conformance). | A pure bytes ABI would break every `get()`. A flat ABI cannot put a `JsValue` in linear memory. | Blocker |
| G3 | **Surface parity.** Schema has 8 methods; the runtime `cache.ts` calls ~55 distinct wasm methods, including query/indexes, ids, drop log, batched raw writes and stats. | A drop-in engine must implement the whole surface used by `cache.ts`, not just `get`/`set`. | Blocker |
| G4 | **Distribution.** No `package.json`, no generated bindings, no asset pipeline, no browser loader. | Nothing installable exists for "our projects". | Blocker |
| G5 | **Safety & versioning.** `memory.grow` detaches JS views; host pointers must stay untrusted; schema fields (`EvictionPolicy`, `Stats`) already disagree with the real engine. | Stale views, unsound reads or silent misdecoding once real values ship. | High |

---

## 3. Decisions to settle before Phase 1 (ADR queue)

Each is a small ADR under `docs/decisions/`, decided by the maintainers, with a recommendation
here so implementation is not blocked.

| ID | Decision | Options | Recommendation | Needed by |
|---|---|---|---|---|
| D1 | Where does the engine come from? | (a) extract `blaze-cache-core` into its own published crate that both repos depend on; (b) vendor a pinned snapshot here + a sync script; (c) reimplement a minimal engine | **(a)**, with **(b)** as the short-term bridge if extraction cannot land first. Reimplementing is last resort: it doubles the bug surface. | Phase 1 |
| D2 | How are JS object values represented? | (a) bytes-only views; (b) integer handles into a JS-side `Map` + `FinalizationRegistry`; (c) externref table via raw ABI | **(b)**: add `alias Ref = handle<u32>` to the schema. Wasm stores handles, JS owns objects → identity preserved, no serialization. Keep `view<buffer>` for binary mode. Document handle lifetime and GC. | Phase 1 |
| D3 | Schema version for the real API | (a) amend `blaze.loom` before tagging 0.1.0; (b) tag 0.1.0 unchanged, introduce `loom-abi-v2` with `ABI_VERSION = 1` in 0.2.0 | **(b)**. The version check exists precisely for this; rewriting a tagged decision artifact is worse than one loud bump. | Phase 0/1 |
| D4 | How does `get()` (async, `[cancel]`) work? | (a) async in wasm; (b) TS orchestration over `get_sync` + host loader + `set`, `AbortSignal` in JS | **(b)**. Keeps wasm sync-only, avoids asyncify, makes cancellation a JS concern where `AbortSignal` already exists. Document that the schema's `async`/`effects` lowers to the TS runtime, not to wasm. | Phase 2 |
| D5 | How is it shipped? | (a) new npm package from this repo, monorepo depends on it; (b) generated artifacts copied into `packages/core/pkg`, no new package | **(a)** primary, with a build script that can also drop artifacts into the monorepo during development. One published source of truth; (b) as debugging fallback only. | Phase 2/4 |
| D6 | Eviction policy mismatch (`Lru/Lfu/Fifo` vs SIEVE) | (a) add `Sieve` as default; (b) map enum onto SIEVE and lie in the schema | **(a)**. The schema must describe what ships; an inaccurate contract is worse than an extra enum variant. | Phase 1 |
| D7 | Stats shape (5 fields in schema vs 8 in core + query/persistence extras) | (a) schema superset with versioned extension; (b) shrink core stats | **(a)**. Additive fields are safe; never regress existing consumers. | Phase 1 |
| D8 | Rollback strategy | (a) engine switch + feature flag, legacy engine retained for one release; (b) hard cutover | **(a)**. Adoption must be reversible; delete the legacy path only after a full release cycle green. | Phase 4/5 |

---

## 4. Roadmap

```
Phase 0  close 0.1.0                 (S)  ─┐
Phase 1  0.2.0 engine + ABI v2        (L)   ├─ blocked by D1–D3, D6, D7
Phase 2  0.3.0 generator + npm pkg    (L)   ─┐ blocked by D2, D4, D5
Phase 3  0.4.0 full parity + browser  (L)   ─┤ blocked by Phase 1+2
Phase 4  0.5.0 monorepo integration   (L)   ─┤ blocked by Phase 3 + D8
Phase 5  0.6.0 rollout / cleanup      (M)   ─┘
```

### Phase 0 — Close and tag 0.1.0 (S)

**Goal:** a clean, immutable baseline so later ABI changes are explicit rather than silent edits.

1. Run `sh scripts/pre-publish.sh`; confirm every check is green (the only unchecked box in the
   0.1.0 plan is the tag itself).
2. Tag `0.1.0`, push the tag. Keep the tag even though the API is foundation-only — it is the
   anchor for `loom-abi-v1`.
3. Add a short note to `docs/plan/0.1.0.md` recording the tag commit.
4. **Do not** amend `blaze.loom` in place (D3).

**Exit:** tag exists; `cargo package --list` excludes `spike/`; CI green on the tagged commit.

---

### Phase 1 — 0.2.0: real ABI + engine behind the flat surface (L)

**Goal:** the crate can actually create a cache and answer `get`/`set`/`has`/`delete`/`stats`
through the flat ABI, with no binding dependency and no unsafe blocks.

**Tasks**

1. **Settle D1, D2, D3, D6, D7** as ADRs (`docs/decisions/0002-engine-source.md`,
   `0003-value-representation.md`, `0004-abi-v2.md` as needed). Update
   `docs/README.md` index in the same change.
2. **Engine source (D1).** Extract or vendor `blaze-cache-core` so this repo compiles the engine
   with zero dependencies on `../blaze-cache/crates/*`. Add `scripts/sync-core.sh` (vendor path)
   or a `Cargo.toml` dependency + version pin (extract path). Record the exact upstream commit in
   `docs/decisions/engine-provenance.md`.
3. **Write `loom-abi-v2`** in `docs/decisions/blaze.loom` (or a new `blaze-v2.loom` with v1 kept
   for history): full `Cache` resource, `Ref` handles, `view<buffer>` with a generation counter,
   `Sieve`, superset `Stats`, explicit error codes. Bump `ABI_VERSION = 1`; keep
   `__blaze_abi_version()` as the first call. Update ADR-0001 with a "v2" section rather than
   rewriting its v1 reasoning.
4. **Flat exports (safe copy-in, per ADR-0001 Approach B).** At minimum:
   `__blaze_abi_version`, `memory`, `__blaze_arena_alloc(len) -> ptr`,
   `__blaze_cache_open(config_ptr, config_len) -> id`,
   `__blaze_cache_get_sync(id, key_ptr, key_len, out_ptr, out_cap) -> i32`,
   `__blaze_cache_set(id, key_ptr, key_len, kind, value_ref_or_ptr, value_len, ttl_ms, tags_ptr, tags_len) -> i32`,
   `__blaze_cache_has/delete/stats/close`, plus `__blaze_generation(id) -> u64`.
   Every host pointer/length is bounds-checked in safe Rust against the arena before any read;
   oversized or reentrant writes return an error code, never grow unchecked.
5. **Error code contract.** One stable `i32` enum (e.g. `0 = OK`, `1 = MISS`, `2 = BAD_ARG`,
   `3 = STALE_VIEW`, `4 = CLOSED`, `5 = OOM`), documented in the schema and asserted by tests.
   No panic crosses the boundary; release still uses `panic = "abort"`.
6. **Host-input fuzz tests.** Random `u32` pointers/lengths, zero-length keys, max-length keys,
   reentrant alloc — property tests in `tests/abi.rs` and Rust unit tests.
7. **Generation/view rule.** Every mutation increments a per-cache generation; the schema and
   docs state that a returned view is valid until the next mutation or `memory.grow`, and the
   JS runtime (Phase 2) must check `__blaze_generation` before dereferencing.

**Files:** `Cargo.toml`, `src/lib.rs` (+ `src/cache.rs`, `src/arena.rs`, `src/abi.rs`),
`tests/abi.rs`, `docs/decisions/blaze.loom`, new ADRs, `scripts/sync-core.sh`.

**Exit criteria**

- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all --check` green.
- `cargo build --release --target wasm32-unknown-unknown` + `wasm-opt -Oz`; ABI check updated
  to expect `1` and still fails loudly on skew.
- A JS harness can instantiate the module, call `__blaze_arena_alloc`, write a key, `set`,
  `get_sync`, `has`, `delete`, `stats`, `close` — with no wasm-bindgen and no unsafe in Rust.
- No dependency on `../blaze-cache/crates/*` (verify with `cargo tree`).

---

### Phase 2 — 0.3.0: Loom generator, TypeScript runtime, npm package (L)

**Goal:** turn the Phase 1 raw exports into something a project can `import` and an app can
build — the first point where "we can use this wasm" is literally true for a greenfield project.

**Tasks**

1. **Settle D4, D5** as ADRs.
2. **Graduate the spike generator.** Promote `spike/m3/loom/generate.mjs` into
   `tools/loom-generate.mjs`: parse `blaze.loom` (not just raw `export` lines), emit
   `bindings.mjs` + `index.d.ts` from one schema, embed the expected ABI version, and **fail the
   build on unknown/missing exports** (the contract is the schema, not the tool).
3. **JS/TS runtime** (`js/runtime/`):
   - `instantiate(source, { expectedAbi })` → fetches/compiles/instantiates, calls
     `__blaze_abi_version()` and throws on mismatch (reuse `js/abi-check.mjs` logic as a library).
   - `BlazeCache` class: `open/getSync/get/set/has/delete/stats/close` matching the schema.
   - Key encoder into the arena (ASCII fast path like `KeyScratch`, UTF-8 `encodeInto` fallback).
   - `view<buffer>` decoder that rebuilds `Uint8Array` whenever `memory.buffer` changes and
     rejects stale views using `__blaze_generation` → `LoomError('StaleView')`.
   - `Ref` handle store (`Map<u32, unknown>` + `FinalizationRegistry`) preserving by-reference
     identity for JS objects (D2).
   - `get()` orchestration with `AbortSignal` (D4), implemented entirely in TS.
   - Error-code → typed error mapping; no silent `undefined` on ABI errors.
4. **Packaging.** Add `package.json` (ESM-only, `type: module`, `sideEffects: false`, typed
   `exports`), a `build` script that runs `cargo build --release --target wasm32-unknown-unknown`
   → `wasm-opt -Oz` → `node tools/loom-generate.mjs` → `tsc` → copy wasm/assets into `dist/`.
   Pin dev deps (`typescript`, `binaryen`/`wasm-opt`, `playwright` for Phase 3 tests).
5. **Publishing gate.** Extend `scripts/pre-publish.sh` to include generation + a `npm pack
   --dry-run` content check; keep the crates.io dry run. CI adds a Node matrix (LTS versions).
6. **Greenfield example.** A tiny Vite + Node example under `examples/` importing the package,
   doing object and bytes round-trips, and forcing a `memory.grow` to prove view refresh.

**Exit criteria**

- `npm pack` contains `.wasm`, `bindings.mjs`, `index.d.ts`, `LICENSE`, README.
- Installing the tarball into a fresh Vite project and a plain Node ESM project works with no
  bundler plugin and no wasm-bindgen dependency.
- ABI skew, stale view, oversized key and `memory.grow` each fail or recover in the documented
  way, covered by tests.
- Generated output is deterministic: regeneration on a clean checkout produces no diff.

---

### Phase 3 — 0.4.0: full runtime parity and browser proof (L)

**Goal:** close G3. Match every wasm method `packages/core/src/cache.ts` actually calls, so the
monorepo can be switched without rewriting it.

**Tasks**

1. **Inventory → contract.** Turn the ~55 wasm-bindgen methods used by `cache.ts` into a parity
   matrix in `docs/parity.md`: method, flat-ABI lowering, schema entry, test source, status.
   Group them: core get/set/delete/has/peek/ttl/touch; batch `getMany/setMany/hasMany/deleteMany`;
   raw key/tag scratch paths; ids (`setMeta/getById/recordMiss/...`); tags (`keysWithTag/tagsOf/
   hasTag/invalidateTags`); bytes (`bytesReserve/setBytes/getBytes/getBytesCopy/wasmMemory`);
   indexes/query (`enableIndexes/queryKeys/queryCount/entryMeta/entriesMeta`); events
   (`setDropTracking/drainDrops`); stats (`stats/resetStats/size/capacity`); lifecycle
   (`clear/prune/free`).
2. **Implement in phases inside this phase:** core + batch → ids + tags → bytes → indexes/query
   → drop log + stats. Each group lands with its own parity rows green, not all at once.
3. **Schema follows code.** Extend `blaze.loom` for every new operation; bump `ABI_VERSION`
   again (to `2`) if any signature changes — no silent evolution. Mark `loom-abi-v2` frozen only
   at the end of this phase.
4. **Port the JS scratch logic.** The proven `KeyScratch`/`TagScratch` technique from
   `packages/core/src/wasm.ts` (one encode per distinct key, flat tag buffers, view rebuild on
   `memory.grow`) belongs in the generated runtime, not in the monorepo adapter. Keep its
   "degrade to the ordinary path when exports are missing" behavior for version skew.
5. **Browser matrix.** Playwright against Chromium/Firefox/WebKit: instantiate, object identity
   across unrelated writes, bytes view after forced growth, tag invalidation, query paging,
   drop-log event delivery, 1 KB set, teardown with zero listeners. M3's limitation (Node/V8
   only) is resolved here.
6. **Performance gate.** Re-run an extended M3 harness using the real engine and compare to the
   current wasm-bindgen engine: `get_sync`, 1 KB `set`, tag invalidate, query page, cold start,
   glue bytes. Publish results in `docs/decisions/adoption-benchmarks.md`. Set budgets before
   measuring (e.g. no >20% regression on hot paths; glue stays <5 KB).
7. **Security review.** Re-run the monorepo's security/SBOM tooling against the new dependency
   graph; review arena bounds, handle reuse and redaction of keys that carry secrets.

**Exit criteria**

- Every row in `docs/parity.md` is green; `docs/parity.md` is generated-checked where possible.
- The monorepo's own `packages/core/test/*` suites can be run against this engine through the
  adapter stub (Phase 4 prepares the switch; Phase 3 proves behavior in isolation).
- Browser matrix green on all three engines; perf budgets met or an ADR records the exception.
- `ABI_VERSION = 2` frozen and checked everywhere.

---

### Phase 4 — 0.5.0: integrate into the monorepo behind a switch (L)

**Goal:** close the loop with existing projects: same public API, new engine.

**Tasks**

1. **Settle D8.** Add `engine?: 'legacy' | 'loom'` to the internal `createCache` options
   (`packages/core/src/cache.ts`) — default stays `'legacy'` until rollout.
2. **Add the dependency:** `@blaze-cache/wasm` as a workspace/lockfile dependency; update
   `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `tsconfig.base.json` path aliases if needed.
3. **Adapter** `packages/core/src/wasm-loom.ts`: implement the same internal interface
   `wasm.ts` exposes today (`loadBlazeWasm`, `keyScratch`, `tagScratch`, `wasmLinearMemory`,
   `WasmCache`) on top of the generated runtime. Keep `wasm.ts` untouched as `wasm-legacy.ts`
   (rename or re-export) so both engines coexist.
4. **Loader parity:** preserve the four source options (`wasmUrl`, `module`, `instantiate`,
   `wasm`), the single-flight loading promise, retry-on-failure and actionable error messages
   from today's `wasm.ts`. This is a behavioral contract, not a nicety — `packages/core/test/
   wasm-*.test.mjs` covers it.
5. **Build scripts:** add `build:wasm:loom` in `../blaze-cache/package.json`;
   `build:wasm` continues to produce the legacy artifact. Add a script that runs both suites.
6. **Run everything:** `pnpm build:wasm && pnpm -r test`, `pnpm typecheck`, `pnpm lint`,
   `pnpm bench:ci` (must match the 0.5.0 baseline detached, per the monorepo rc gate), all 7 Vite
   examples, browser suite. Compare results between engines.
7. **Feature-flagged dogfood:** switch one example (start with `examples/core`) to
   `engine: 'loom'`; keep it green for a full release cycle.

**Exit criteria**

- With `engine: 'loom'`, all core, package and example tests pass unchanged.
- No public `@blaze-cache/core` export changes; `index.ts` gains no new required arguments.
- Bundle-size budget: the loom build of core is no worse than the agreed budget from Phase 3.
- A single flag flips an example between engines; rollback is one edit plus a rebuild.

---

### Phase 5 — 0.6.0: rollout, then remove the legacy path (M)

**Goal:** make the new engine the default, then retire the old one—without breaking consumers.

1. Flip the default to `'loom'`; keep `'legacy'` as an explicit opt-out for one release.
2. Update monorepo docs, READMEs, upgrade notes and `docs/README.md` index; document the engine
   option, ABI version, view lifetime and how to diagnose skew.
3. Publish the new package and the monorepo release together; verify install order.
4. After one full release cycle green, remove `crates/blaze-cache-wasm` + `packages/core/pkg`
   glue generation and the `'legacy'` branch — in a **breaking** monorepo version, never a patch.
5. Delete `spike/` from the published crate (already excluded) but keep it in the repo as
   benchmark provenance; archive `blaze.loom` v1/v2 under `docs/decisions/history/`.

**Exit:** the new engine is the only engine; no consumer depends on wasm-bindgen glue; the
legacy removal is recorded as an upgrade note.

---

## 5. Verification strategy (applies to every phase)

| Layer | Tool | Gate |
|---|---|---|
| Rust unit | `cargo test --locked` | Every commit; host target |
| Rust lint/format | `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` | Every commit |
| Wasm build | `cargo build --release --target wasm32-unknown-unknown` + `wasm-opt -Oz` | Every commit |
| ABI | `node js/abi-check.mjs` (extended for v2+: export set, error codes, generation) | Every commit; skew must throw |
| Fuzz/property | random host pointers/lengths, reentrant alloc, max keys | Phase 1 onward |
| Generated bindings | regeneration diff check + export-presence assertion | Phase 2 onward |
| JS unit/integration | `node:test` in `js/test/` and this repo's `npm test` | Phase 2 onward |
| Cross-runtime | Node LTS matrix + Chromium/Firefox/WebKit via Playwright | Phase 3 onward |
| Parity | `docs/parity.md` + ported `packages/core/test/*` against the adapter | Phase 3–4 |
| Performance | extended M3 harness + `pnpm bench:ci` baseline | Phase 3–4 |
| Supply chain | dependency audit, SBOM regeneration, key-redaction review | Phase 3 and release |

**Invariants that must hold at every phase**

- No `unsafe` blocks/calls in Rust; the only lint exception is the documented flat export.
- No host pointer is ever trusted; input is copied into the arena and bounds-checked in safe Rust.
- ABI version is checked before any other call; a mismatch throws rather than misdecodes.
- Views are never trusted across a mutation or a `memory.buffer` swap.
- Generated files are never hand-edited; the schema is the contract.

---

## 6. CI, release and documentation housekeeping

- Extend `.github/workflows/ci.yml` from the Rust-only gate to a two-job gate: **Rust** (existing
  pre-publish script) and **JS** (install, `npm run build`, `npm test`, tarball dry run). Add a
  browser job in Phase 3.
- Keep `scripts/pre-publish.sh` as the single local gate; CI must run the same commands, not a
  near-copy. Extend it, don't fork it.
- Keep `.githooks/pre-push` aligned when new checks land (currently fmt/clippy/test/wasm/ABI).
- Version the npm package and the crate together; record both versions in release notes and in
  `ABI_VERSION` (one schema version = one crate version = one npm version).
- Every phase updates `docs/README.md` and the root `README.md` Status/layout sections in the
  same change as the code — this repo's own convention (see `docs/README.md` Conventions).
- Add `docs/decisions/adoption-benchmarks.md` (Phase 3) and `docs/parity.md` (Phase 3) to the
  index.

---

## 7. Migration playbook — what a project actually does at the end

**Greenfield (new project, Phase 2+):** install `@blaze-cache/wasm`, instantiate once, call the
typed API.

```ts
import { createBlazeCache } from '@blaze-cache/wasm';

const cache = await createBlazeCache({ wasmUrl: '/assets/blaze_cache_wasm.wasm' });
cache.set('user:42', { id: 42, name: 'Ada' }, { ttlMs: 60_000 });
const user = cache.get('user:42'); // same object reference
const bytes = cache.getBytes('blob:1'); // Uint8Array view, valid until next mutation/growth
cache.close();
```

**Existing `@blaze-cache/core` consumer (Phase 4+):** no code change except the engine flag
during the trial; public API is identical.

```ts
import { createCache } from '@blaze-cache/core';

const cache = await createCache({ capacity: 10_000, engine: 'loom' }); // 'legacy' = old engine
// every existing method — get/set/query/invalidateTags/persistence/events — behaves the same
```

**Rollback:** set `engine: 'legacy'`, rebuild, redeploy. No data migration: the engine is an
implementation detail; IndexedDB persistence and snapshots are written by the TS layer.

> The exact exported names above are the **target shape** and will be fixed by D2/D5 and the
> Phase 2 generated types. They are illustrative here, not a promise to preserve a typo.

---

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Engine extraction stalls (D1) | Vendor a pinned snapshot now behind `scripts/sync-core.sh`; extraction becomes a dependency bump, not a rewrite. |
| Handle table leaks JS objects (D2) | `FinalizationRegistry` + explicit `close()` invalidation + leak test in Phase 3 browser matrix. |
| View detachment causes subtle bugs (G5) | Generation counter checked in the runtime; forced-`memory.grow` test in every runtime; contract in schema and docs. |
| parity is larger than estimated (G3) | `docs/parity.md` inventory first; each group lands independently; no big-bang cutover. |
| Performance regresses vs wasm-bindgen | Budgets fixed before measurement (Phase 3); generated glue is small by construction; ADR required for any exception. |
| Two engines drift in behavior | Run the same core test suite against both in Phase 4; keep `'legacy'` for one release as an oracle. |
| Schema drift / silent ABI change | Every signature change bumps `ABI_VERSION`; regeneration diff check; ADR queue is mandatory before Phase 1. |
| Monorepo is mid-0.5.0 (branch `v0.5.0-beta.1`) | Phase 4 integration starts only after the monorepo's `0.5.0-rc.1` baseline is confirmed, per its own go/no-go gate. Do not couple this roadmap to an unstable core. |

---

## 9. Definition of done — the full adoption checklist

- [ ] `0.1.0` tagged; v1 schema and M3 evidence immutable in history.
- [ ] D1–D8 decided and recorded as ADRs; `docs/README.md` index updated.
- [ ] Engine builds in this repo with no dependency on `../blaze-cache/crates/*`.
- [ ] `loom-abi-v2` (and later v3) schema committed; `ABI_VERSION` bumped and checked in Rust, JS and CI.
- [ ] Flat cache API implemented with safe copy-in, error codes, generation counter and fuzz tests.
- [ ] Loom generator emits deterministic JS + TS from the schema; no generated file is hand-edited.
- [ ] npm package published and installable in a fresh Vite and Node ESM project.
- [ ] `docs/parity.md` fully green; all monorepo wasm methods used by `cache.ts` have flat equivalents.
- [ ] Browser matrix (Chromium/Firefox/WebKit) green; perf budgets met or excepted by ADR.
- [ ] `@blaze-cache/core` runs unchanged on the new engine behind `engine: 'loom'`; all tests,
      examples, typecheck, lint and `bench:ci` green.
- [ ] Rollback demonstrated; legacy engine retained for one release, then removed in a breaking version.
- [ ] Docs, upgrade notes, SBOM and security assessment updated in the same release as the code.

---

## 10. Immediate next actions (this week)

1. Decide D3 first — it is a 10-minute decision that unblocks the tag and the v2 plan.
2. Open ADRs for D1 and D2; these are the two with real design content and dominate Phase 1.
3. Run `sh scripts/pre-publish.sh` and tag `0.1.0` (Phase 0) before touching the schema.
4. Create `docs/parity.md` skeleton from the method inventory in §4 Phase 3, so the true size of
   G3 is visible before anyone estimates Phase 1–4.
