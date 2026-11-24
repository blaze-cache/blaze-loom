# Documentation

Project-level documents for the independent `blaze-cache-wasm` crate. The root
[`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) governs all community participation.

## Index

| Category | Document | Version / date | Description |
|---|---|---|---|
| Plan | [v0.1.0 project plan](plan/0.1.0.md) | `0.1.0`, 2026-11-22 | Foundation scope, ABI decision, milestones, risks and acceptance criteria. |
| Decision | [ADR-0001: Wasm ↔ JS ABI strategy](decisions/0001-abi-strategy.md) | 2026-11-22 | Flat raw ABI + schema generator, confirmed by M3; safety tradeoffs and the view rule. |
| Evidence | [M3 ABI spike report](decisions/m3-spike.md) | 2026-11-24 | Four-arm benchmarks: size, cold start, call cost and 1 KB set cost. |
| Schema | [`blaze.loom`](decisions/blaze.loom) | `loom-abi-v1` | The ABI contract as a schema: linear `Cache`, opaque `Key`, view `Value`. |

## Conventions

- Name planning documents after the release they target (`plan/<version>.md`).
- Record the version and date in every document header.
- Keep this index updated in the same change that adds a document.
