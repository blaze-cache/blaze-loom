# Documentation

Project-level documents for the independent `blaze-cache-wasm` crate. The root
[`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) governs all community participation.

## Index

| Category | Document | Version / date | Description |
|---|---|---|---|
| Plan | [Roadmap to adoption](plan/ADOPTION-ROADMAP.md) | 2026-11-24 | Full path from the 0.1.0 foundation to a published, drop-in engine for our projects: decisions, phases, parity, tests and rollback. |
| Plan | [v0.1.0 project plan](plan/0.1.0.md) | `0.1.0`, 2026-11-22 (tagged 2026-11-24) | Foundation scope, ABI decision, milestones, risks, acceptance criteria and the tagged release record. |
| Plan | [v0.2.0 project plan](plan/0.2.0.md) | `0.2.0`, 2026-11-24 (tagged 2026-11-25) | Phase 1: engine source, ADR queue, `loom-abi-v2`, flat cache exports and the 0.2.0 gate. |
| Decision | [ADR-0001: Wasm ↔ JS ABI strategy](decisions/0001-abi-strategy.md) | 2026-11-22 (v2 section 2026-11-24) | Flat raw ABI + schema generator, confirmed by M3; safety tradeoffs, view rule, and the 0.2.0 v2 pointer. |
| Decision | [ADR-0002: Engine source (D1)](decisions/0002-engine-source.md) | 2026-11-24 | Vendor a pinned `blaze-cache-core` snapshot behind `scripts/sync-core.sh`; extraction stays the target; never reimplement. |
| Decision | [ADR-0003: Value representation (D2)](decisions/0003-value-representation.md) | 2026-11-24 | `Ref = handle<u32>` + JS-owned `Map` for objects; `view<buffer>` kept for bytes; handle lifetime documented. |
| Decision | [ADR-0004: loom-abi-v2 (D3, D6, D7)](decisions/0004-abi-v2.md) | 2026-11-24 | New `blaze-v2.loom` (v1 frozen), truthful `Sieve`-only enum, 7-field superset `Stats`. |
| Evidence | [Engine provenance](decisions/engine-provenance.md) | `bf0bf278`, 2026-11-24 | Pinned upstream commit, inlined manifest values, dependency pins and the sync procedure. |
| Evidence | [M3 ABI spike report](decisions/m3-spike.md) | 2026-11-24 | Four-arm benchmarks: size, cold start, call cost and 1 KB set cost. |
| Schema | [`blaze.loom`](decisions/blaze.loom) | `loom-abi-v1` | Frozen 0.1.0 contract as tagged: linear `Cache`, opaque `Key`, view `Value`. |
| Schema | [`blaze-v2.loom`](decisions/blaze-v2.loom) | `loom-abi-v2` | 0.2.0 contract: config, value kinds, tags, 7-field stats, error codes, generation. |
| Contract | [`parity.md`](parity.md) | skeleton, 2026-11-24 | Method-by-method parity matrix vs `cache.ts` (Phase 3 fills it). |

## Conventions

- Name planning documents after the release they target (`plan/<version>.md`).
- Record the version and date in every document header.
- Keep this index updated in the same change that adds a document.
