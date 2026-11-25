/**
 * ABI contract check for the flat raw ABI — `loom-abi-v2` (0.2.0).
 *
 * There is no generated glue and no npm package: this script instantiates the raw `.wasm`
 * produced by `cargo build --target wasm32-unknown-unknown --release` with an empty import
 * object and fails loudly when the host ABI does not match what the plan pins:
 *
 *   1. the export set is exactly loom-abi-v2's (no missing, no extra exports),
 *   2. `__blaze_abi_version()` reports `1`,
 *   3. the behavioural error codes are stable (BAD_ARG / MISS / CLOSED …),
 *   4. `__blaze_generation` exists and behaves (0 for unknown ids, +1 per mutation),
 *   5. a deliberately wrong expected version throws instead of misdecoding.
 *
 * This file is also the **reference host** for the epoch contract: every operation
 * allocates *all* of its inputs in one epoch (no `alloc` while pointers from that epoch
 * are still live), calls exactly one arena-touching export, then reads the results. Byte
 * round-trips (stats fields, value bytes) live in `js/harness.mjs`; this file pins the
 * contract.
 *
 * Run: node js/abi-check.mjs [path/to/blaze_cache_wasm.wasm]
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

/** Expected value of `ABI_VERSION` — the Rust constant is pinned to the same number. */
const LOOM_ABI_EXPECTED = 1;

/** The exact export surface `loom-abi-v2` pins (plan § "Exports"). */
const EXPECTED_EXPORTS = [
  '__blaze_abi_version',
  '__blaze_arena_alloc',
  '__blaze_cache_close',
  '__blaze_cache_delete',
  '__blaze_cache_get_sync',
  '__blaze_cache_has',
  '__blaze_cache_open',
  '__blaze_cache_set',
  '__blaze_cache_stats',
  '__blaze_generation',
  '__blaze_tick',
  'memory',
].sort();

/** Error codes as the wire sees them: exports return `-code`, `OK` is `0`. */
const OK = 0;
const MISS = -1;
const BAD_ARG = -2;
const CLOSED = -4;

/** Kind `0` = Ref (a `u32` handle), kind `1` = Bytes (arena pointer + length). */
const KIND_REF = 0;
const KIND_BYTES = 1;

/** A pointer no bounds check can accept on any target. */
const DANGLING = 0xffff_fff0;

/**
 * Check the module's exported ABI version against `expected`, failing loudly on skew.
 * @param {WebAssembly.Instance} instance
 * @param {number} expected
 * @returns {number} the host's ABI version
 */
function checkAbi(instance, expected) {
  const hostAbi = instance.exports.__blaze_abi_version?.();

  assert.equal(
    typeof hostAbi,
    'number',
    'the wasm module must export __blaze_abi_version()',
  );
  assert.equal(
    hostAbi,
    expected,
    `ABI version mismatch: host=${hostAbi} expected=${expected}`,
  );

  return hostAbi;
}

const path = resolve(
  process.argv[2] ?? 'target/wasm32-unknown-unknown/release/blaze_cache_wasm.wasm',
);
const bytes = await readFile(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const wasm = instance.exports;

// ── 1. the export set is exactly what loom-abi-v2 promises ─────────────────────
const names = Object.keys(wasm).sort();
assert.deepEqual(
  names,
  EXPECTED_EXPORTS,
  `export set drifted from loom-abi-v2:\n  got:  ${names.join(', ')}\n  want: ${EXPECTED_EXPORTS.join(', ')}`,
);

// ── 2. ABI_VERSION = 1, and a wrong expectation must throw ─────────────────────
assert.throws(
  () => checkAbi(instance, 999),
  /ABI version mismatch/,
  'a deliberately wrong expected version must throw, not misdecode',
);
const hostAbi = checkAbi(instance, LOOM_ABI_EXPECTED);

// ── host helpers (the epoch contract in code) ──────────────────────────────────
const u8 = (ptr, len) => new Uint8Array(wasm.memory.buffer, ptr, len);
const u32 = (ptr) => new Uint32Array(wasm.memory.buffer, ptr, 1)[0];

/** Allocate `len` zeroed scratch bytes; `0` (the OOM signal) fails the check. */
function alloc(len) {
  const ptr = wasm.__blaze_arena_alloc(len);
  assert.notEqual(ptr, 0, `arena_alloc(${len}) must succeed`);
  return ptr;
}

/** Allocate scratch and copy `data` into it (same epoch as any other live `alloc`). */
function scratch(data) {
  const ptr = alloc(data.length);
  u8(ptr, data.length).set(data);
  return ptr;
}

/** Open a cache from an 8-byte little-endian config `{capacity, eviction}`. */
function openConfig(capacity, eviction) {
  const cfg = scratch([
    capacity & 0xff, (capacity >>> 8) & 0xff, (capacity >>> 16) & 0xff, (capacity >>> 24) & 0xff,
    eviction & 0xff, (eviction >>> 8) & 0xff, (eviction >>> 16) & 0xff, (eviction >>> 24) & 0xff,
  ]);
  return wasm.__blaze_cache_open(cfg, 8);
}

// ── 3. behavioural error codes ────────────────────────────────────────────────

// Bad configs: a wrong length is BAD_ARG, never a trap.
{
  const opened = openConfig(0, 0);
  assert.ok(opened > 0, `the zero config must open (capacity clamps to 1), got ${opened}`);
  const short = alloc(4);
  assert.equal(wasm.__blaze_cache_open(short, 4), BAD_ARG, 'a 4-byte config is BAD_ARG');
  assert.equal(wasm.__blaze_cache_open(0, 0), BAD_ARG, 'an empty config is BAD_ARG');
  assert.equal(wasm.__blaze_cache_open(DANGLING, 8), BAD_ARG, 'a dangling config pointer is BAD_ARG');
}

// Unknown ids are BAD_ARG; a missing key is MISS. Every input for this whole block is
// allocated up front — the ops below arm the epoch but never allocate, so `key` and the
// output buffers stay live for all of them.
const id = openConfig(4, 0);
assert.ok(id > 0, `open must return an id, got ${id}`);
{
  const key = scratch([107]); // "k"
  const lenProbe = alloc(4);
  const statsBuf = alloc(56);

  assert.equal(wasm.__blaze_cache_set(9999, key, 1, KIND_REF, 7, 0, 0n, 0, 0), BAD_ARG, 'set on an unknown id');
  assert.equal(wasm.__blaze_cache_has(9999, key, 1), BAD_ARG, 'has on an unknown id');
  assert.equal(wasm.__blaze_cache_get_sync(9999, key, 1, 0, 0, lenProbe), BAD_ARG, 'get on an unknown id');
  assert.equal(wasm.__blaze_cache_delete(9999, key, 1), BAD_ARG, 'delete on an unknown id');
  assert.equal(wasm.__blaze_cache_stats(9999, statsBuf, 56), BAD_ARG, 'stats on an unknown id');
  assert.equal(wasm.__blaze_cache_close(9999), BAD_ARG, 'close on an unknown id');
  assert.equal(wasm.__blaze_generation(9999), 0n, 'generation of an unknown id is 0');

  assert.equal(wasm.__blaze_cache_get_sync(id, key, 1, 0, 0, lenProbe), MISS, 'probe of a missing key');
  assert.equal(wasm.__blaze_cache_has(id, key, 1), MISS, 'has of a missing key');
  assert.equal(wasm.__blaze_cache_delete(id, key, 1), MISS, 'delete of a missing key');
}

// Dangling pointers are BAD_ARG on every pointer-taking export. (No live pointer from a
// previous epoch is at stake here — every argument is either dangling or freshly
// allocated in the same call.)
{
  assert.equal(wasm.__blaze_cache_has(id, DANGLING, 4), BAD_ARG, 'dangling key pointer');
  assert.equal(wasm.__blaze_cache_set(id, DANGLING, 4, KIND_REF, 7, 0, 0n, 0, 0), BAD_ARG, 'dangling key pointer in set');
  assert.equal(wasm.__blaze_cache_set(id, 0, 4, KIND_BYTES, DANGLING, 4, 0n, 0, 0), BAD_ARG, 'dangling value pointer in set');
  assert.equal(wasm.__blaze_cache_get_sync(id, DANGLING, 4, 0, 0, alloc(4)), BAD_ARG, 'dangling key pointer in get');
  assert.equal(wasm.__blaze_cache_get_sync(id, 0, 0, DANGLING, 4, DANGLING), BAD_ARG, 'dangling out pointers in get');
  assert.equal(wasm.__blaze_cache_stats(id, DANGLING, 56), BAD_ARG, 'dangling stats pointer');
}

// Unknown value kinds are BAD_ARG; both legal kinds succeed. Value and both keys are
// allocated together, then every `set` below runs without an intervening `alloc`. The
// two legal sets use **different** keys — same key would overwrite Ref with Bytes.
{
  const value = scratch([1, 2, 3]);
  const refKey = scratch([107]); // "k"
  const bytesKey = scratch([115]); // "s"
  for (const kind of [2, 3, 0xffff]) {
    assert.equal(
      wasm.__blaze_cache_set(id, refKey, 1, kind, 7, 0, 0n, 0, 0),
      BAD_ARG,
      `kind ${kind} must be BAD_ARG`,
    );
  }
  assert.equal(wasm.__blaze_cache_set(id, refKey, 1, KIND_REF, 7, 0, 0n, 0, 0), OK, 'a Ref set');
  assert.equal(wasm.__blaze_cache_set(id, bytesKey, 1, KIND_BYTES, value, 3, 0n, 0, 0), OK, 'a Bytes set');
}

// Truncated tag records are BAD_ARG; the empty tag buffer means "no tags". The key is
// re-allocated inside each epoch — the previous `set` armed the reset, so the next
// `alloc` recycles everything.
{
  for (const tagsLen of [1, 2, 3, 5]) {
    const tags = alloc(8);
    const key = scratch([108]); // "l"
    assert.equal(
      wasm.__blaze_cache_set(id, key, 1, KIND_REF, 7, 0, 0n, tags, tagsLen),
      BAD_ARG,
      `tags_len=${tagsLen} must cut a record → BAD_ARG`,
    );
  }
  const key = scratch([108]); // "l"
  assert.equal(
    wasm.__blaze_cache_set(id, key, 1, KIND_REF, 7, 0, 0n, 0, 0),
    OK,
    'an empty tag buffer means no tags',
  );
}

// get_sync: probe mode, hit, and the out buffer must fit the value. All four buffers
// are allocated in one epoch, then the three gets run back to back.
{
  const out = alloc(4);
  const lenPtr = alloc(4);
  const tooSmall = alloc(2);
  const key = scratch([107]); // "k" — stored by the kind block above

  assert.equal(wasm.__blaze_cache_get_sync(id, key, 1, 0, 0, lenPtr), OK, 'probe of a stored key');
  assert.equal(u32(lenPtr), 4, 'a Ref value is always 4 bytes');

  assert.equal(wasm.__blaze_cache_get_sync(id, key, 1, out, 4, lenPtr), OK, 'get of a Ref value');
  assert.equal(u32(lenPtr), 4, 'the written length is 4');

  assert.equal(
    wasm.__blaze_cache_get_sync(id, key, 1, tooSmall, 2, lenPtr),
    BAD_ARG,
    'an out buffer smaller than the value is BAD_ARG',
  );
}

// stats needs room for the full 56-byte struct. Each inline `alloc` consumes the arm
// the previous `stats` left behind, so every buffer is fresh for its own call.
{
  assert.equal(wasm.__blaze_cache_stats(id, alloc(8), 8), BAD_ARG, 'a short stats buffer');
  assert.equal(wasm.__blaze_cache_stats(id, alloc(55), 55), BAD_ARG, '55 < 56 bytes');
  assert.equal(wasm.__blaze_cache_stats(id, alloc(56), 56), OK, 'a full stats buffer');
}

// ── 4. the generation export ──────────────────────────────────────────────────
{
  const fresh = openConfig(4, 0);
  assert.equal(wasm.__blaze_generation(fresh), 1n, 'a fresh cache starts at generation 1');

  const key = scratch([109]); // "m"
  const lenPtr = alloc(4);
  assert.equal(wasm.__blaze_cache_set(fresh, key, 1, KIND_REF, 1, 0, 0n, 0, 0), OK);
  const afterSet = wasm.__blaze_generation(fresh);
  assert.ok(afterSet > 1n, `set must bump the generation, got ${afterSet}`);
  assert.equal(wasm.__blaze_cache_delete(fresh, key, 1), OK);
  const afterDelete = wasm.__blaze_generation(fresh);
  assert.ok(afterDelete > afterSet, `delete must bump the generation, got ${afterDelete}`);

  // close consumes the id: every later op is CLOSED (never BAD_ARG, never OK), and the
  // tombstone keeps reporting its post-close generation.
  assert.equal(wasm.__blaze_cache_close(fresh), OK);
  assert.equal(wasm.__blaze_cache_set(fresh, key, 1, KIND_REF, 1, 0, 0n, 0, 0), CLOSED, 'set on a closed id');
  assert.equal(wasm.__blaze_cache_has(fresh, key, 1), CLOSED, 'has on a closed id');
  assert.equal(wasm.__blaze_cache_delete(fresh, key, 1), CLOSED, 'delete on a closed id');
  assert.equal(wasm.__blaze_cache_get_sync(fresh, key, 1, 0, 0, lenPtr), CLOSED, 'get on a closed id');
  assert.equal(wasm.__blaze_cache_close(fresh), CLOSED, 'double close');
  assert.equal(
    wasm.__blaze_generation(fresh),
    afterDelete + 1n,
    'close bumps the generation and the tombstone keeps reporting it',
  );
}

// ── 5. the host-driven clock export is callable (wasm has no clock of its own) ─
wasm.__blaze_tick(0);
wasm.__blaze_tick(performance.now());

console.log(
  `ABI OK: ABI_VERSION=${hostAbi} (loom-abi-v2), ${names.length} exports, error codes + generation verified (${path})`,
);
