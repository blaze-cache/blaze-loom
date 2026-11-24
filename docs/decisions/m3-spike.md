# M3 ABI spike report

**Version:** `0.1.0`
**Date:** 2026-11-24
**Relates to:** [ADR-0001](0001-abi-strategy.md), [v0.1.0 plan](../plan/0.1.0.md)

## Question

M3 had to confirm or supersede ADR-0001's provisional choice with evidence for four metrics:
binary size, cold start, call cost and 1 KB set cost. Four arms implement the same logical
operations (`set`, `has`, `get_sync`) over an in-memory `HashMap<Vec<u8>, Vec<u8>>`:

1. **flat raw, hand-written** — raw `(ptr, len)` exports, hand-written JS decoder;
2. **flat raw + Loom** — identical wasm core, JS bindings generated at build time from a schema;
3. **generated glue (wasm-bindgen)** — `wasm-bindgen` 0.2.130 via `wasm-pack`;
4. **Component Model / WIT** — `wit-bindgen` 0.63 guest, `wasm-tools` 1.263 componentization,
   `jco` 1.42 transpiled JS runtime.

## Method

Reproduce with `sh spike/m3/build.sh && node spike/m3/harness.mjs` (requires `wasm-pack`,
`wasm-tools`, and the npm dev dependency declared in `spike/m3/package.json`).

* **Environment:** rustc 1.99.1, Node v24.5.0, macOS aarch64; wasm-opt 132.
* **Fairness:** all arms use `opt-level = "z"`, `lto = true`, `panic = "abort"`, then a
  `wasm-opt -Oz` pass. `wasm-pack` applies this automatically; `build.sh` applies the same pass
  to the raw and component cores.
* **Size:** raw `.wasm` bytes plus the JS glue files an app would ship, measured on disk.
* **Cold start:** median of 50 fresh compile + instantiate cycles. The Component Model arm
  measures a fresh import of its transpiled runtime, which is the first-load cost of that arm.
* **Call cost:** median of 5 × 20,000 `get_sync()` calls on an 11-byte key and 16-byte value.
* **1 KB set:** median of 5 × 2,000 `set()` calls with a 1,024-byte value.
* 2,000 warmup iterations before each run. Raw data is in [`spike/m3/results.json`](../../spike/m3/results.json).

## Results

| Arm | wasm B | glue B | cold ms | get_sync µs | set 1 KB µs |
|---|---:|---:|---:|---:|---:|
| flat raw, hand-written | 18,166 | 1,225 | 0.0252 | 0.1986 | 0.3010 |
| flat raw + Loom | 18,166 | 2,276 | 0.0241 | 0.2246 | 0.3239 |
| generated glue (wasm-bindgen) | 18,621 | 6,972 | 0.0249 | 0.1831 | 0.2620 |
| Component Model / WIT (jco) | 19,190 | 110,450 | 1.8008 | 3.2997 | 3.8801 |

## Reading the evidence

* **Core size is a wash:** all four wasm modules land within ~6% of each other. The size story is
  told by the JS glue, not the module: 1.2 KB raw, 2.3 KB Loom, 7 KB wasm-bindgen, 110 KB jco.
* **Loom costs ~1 KB of glue and no measurable runtime penalty.** Its `get_sync` and 1 KB set
  numbers are within run-to-run noise of the hand-written raw arm. The schema-driven generator
  therefore buys ABI discipline essentially for free.
* **Generated glue is not a performance problem.** wasm-bindgen is within noise (and in this run
  marginally faster) on both hot-path metrics. It is not selected for the independence reason in
  ADR-0001 — coupling to one generator's glue — not because it is slow. This is recorded so a
  later reader does not mistake the decision for a benchmark result.
* **The Component Model is decisively worse for this use case:** ~15× the per-call cost, ~13× the
  1 KB set cost, ~70× the cold start and ~90× the glue. This corroborates the ADR's concern about
  browser runtime support rather than merely tooling churn. Revisit only if native browser
  component support and lighter JS runtimes change the picture.

## Limitations

* Microbenchmark of one storage shape on one host (Node/V8), not a browser matrix. Firefox and
  Safari predictions in ADR-0001 remain predictions.
* The store is a `thread_local!` `HashMap`; no eviction, TTL, views, async or cancellation are
  measured. This evidence is about ABI overhead, not cache-engine performance.
* Cold-start definitions differ by necessity: core arms time compile + instantiate, the
  component arm times a full runtime import. That reflects real first-load cost but is not a
  like-for-like measurement of the Wasm engine alone.
* The Loom arm's `spike.loom` describes raw export signatures only. Lowering the full high-level
  `blaze.loom` contract (resources, views, async) is 0.2.0 work and remains unmeasured.
* Core-arm cold-start timings varied between 0.022 ms and 0.054 ms across runs; treat sub-0.03 ms
  differences as noise. The Component Model gap is far outside that noise.

## Decision

M3 **confirms** ADR-0001: keep the flat raw ABI and use Loom as a build-time generator for the
0.2.0 bindings. Do not pick the Component Model for the browser binding layer on current tooling.
