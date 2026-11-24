/**
 * Minimal JS-side ABI version check for the flat raw ABI.
 *
 * There is no generated glue and no npm package at 0.1.0: this script instantiates the raw
 * `.wasm` produced by `cargo build --target wasm32-unknown-unknown` and fails loudly when the
 * host ABI does not match. Later releases will replace the hard-coded check with the
 * `__loom_view()` runtime's own negotiation, but the contract stays the same.
 *
 * Run: node js/abi-check.mjs [path/to/blaze_cache_wasm.wasm]
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

/** Expected value of ABI_VERSION = loom-abi-v1. */
const LOOM_ABI_EXPECTED = 0;

const path = resolve(
  process.argv[2] ?? 'target/wasm32-unknown-unknown/debug/blaze_cache_wasm.wasm',
);
const bytes = await readFile(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const hostAbi = instance.exports.__blaze_abi_version?.();

assert.equal(
  typeof hostAbi,
  'number',
  'the wasm module must export __blaze_abi_version()',
);
assert.equal(
  hostAbi,
  LOOM_ABI_EXPECTED,
  `ABI mismatch: host=${hostAbi} expected=${LOOM_ABI_EXPECTED}`,
);

console.log(`ABI OK: host=${hostAbi} (loom-abi-v1)`);
