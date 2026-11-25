/**
 * End-to-end JS harness for the flat raw ABI — the release module, no generated glue.
 *
 * Where `js/abi-check.mjs` pins the *contract* (export set, version, error codes), this
 * file proves the *data path*: a host instantiates the raw `.wasm`, allocates buffers in
 * linear memory, and round-trips real bytes through the cache:
 *
 *   open → set (a `Ref` handle and a 256-byte value covering every byte value) →
 *   get_sync (probe + read-back, byte-for-byte) → has → stats (7-field decode) →
 *   TTL expiry driven by `__blaze_tick` → delete → close
 *
 * Every operation follows the epoch contract: allocate *all* inputs, call exactly one
 * arena-touching export, read the results, repeat. Views are created from
 * `memory.buffer` at use time — a Rust allocation can grow linear memory, which detaches
 * older views, so nothing caches the buffer across an export call.
 *
 * Run: node js/harness.mjs [path/to/blaze_cache_wasm.wasm]
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const OK = 0;
const MISS = -1;
const CLOSED = -4;
const KIND_REF = 0;
const KIND_BYTES = 1;

/** The seven little-endian `u64` fields of the pinned 56-byte stats struct. */
const STATS_FIELDS = [
  'hits', 'misses', 'evictions', 'expirations', 'invalidations', 'size', 'capacity',
];

const path = resolve(
  process.argv[2] ?? 'target/wasm32-unknown-unknown/release/blaze_cache_wasm.wasm',
);
const bytes = await readFile(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const wasm = instance.exports;

// ── host helpers (the epoch contract in code) ──────────────────────────────────
const u8 = (ptr, len) => new Uint8Array(wasm.memory.buffer, ptr, len);
const u32 = (ptr) => new Uint32Array(wasm.memory.buffer, ptr, 1)[0];
const u64 = (ptr) => new BigUint64Array(wasm.memory.buffer, ptr, 1)[0];

/** Allocate `len` scratch bytes (the arena zeroes them; `0` is the OOM signal). */
function alloc(len) {
  const ptr = wasm.__blaze_arena_alloc(len);
  assert.notEqual(ptr, 0, `arena_alloc(${len}) must succeed`);
  return ptr;
}

/** Allocate scratch and copy `data` into it. */
function scratch(data) {
  const ptr = alloc(data.length);
  u8(ptr, data.length).set(data);
  return ptr;
}

/** Encode `{capacity, eviction}` as the plan's 8-byte little-endian config. */
function scratchConfig(capacity, eviction) {
  const buf = new ArrayBuffer(8);
  new DataView(buf).setUint32(0, capacity, true);
  new DataView(buf).setUint32(4, eviction, true);
  return scratch(new Uint8Array(buf));
}

/** Open a cache; returns its id (ids start at 1). */
function open(capacity) {
  return wasm.__blaze_cache_open(scratchConfig(capacity, 0), 8);
}

/** Set `key` to a `Ref` handle (kind 0) or bytes (kind 1), with optional TTL/tags. */
function set(id, keyBytes, { kind, ref = 0, value, ttlMs = 0n, tags = [] } = {}) {
  // All inputs in one epoch: tag buffer first, then key + value.
  const tagsFlat = [];
  for (const tag of tags) {
    const encoded = new TextEncoder().encode(tag);
    const len = new ArrayBuffer(4);
    new DataView(len).setUint32(0, encoded.byteLength, true);
    tagsFlat.push(new Uint8Array(len), encoded);
  }
  const tagsPtr = tagsFlat.length ? scratch(concat(tagsFlat)) : 0;
  const tagsLen = tagsFlat.length ? tagsFlat.reduce((n, p) => n + p.length, 0) : 0;

  const key = scratch(keyBytes);
  if (kind === KIND_BYTES) {
    const val = scratch(value);
    return wasm.__blaze_cache_set(id, key, keyBytes.length, kind, val, value.length, ttlMs, tagsPtr, tagsLen);
  }
  return wasm.__blaze_cache_set(id, key, keyBytes.length, kind, ref, 0, ttlMs, tagsPtr, tagsLen);
}

/** `get_sync` with a fresh key + output + length buffer; returns `{code, len, bytes}`. */
function get(id, keyBytes, outCap) {
  const key = scratch(keyBytes);
  const out = outCap > 0 ? alloc(outCap) : 0;
  const outCapActual = outCap > 0 ? outCap : 0;
  const lenPtr = alloc(4);
  const code = wasm.__blaze_cache_get_sync(
    id, key, keyBytes.length, out, outCapActual, lenPtr,
  );
  const len = u32(lenPtr);
  const value = code === OK && outCapActual > 0 ? u8(out, Math.min(len, outCapActual)).slice() : null;
  return { code, len, bytes: value };
}

/** `get_sync` in probe mode: only the length word is written. */
function probe(id, keyBytes) {
  const key = scratch(keyBytes);
  const lenPtr = alloc(4);
  const code = wasm.__blaze_cache_get_sync(id, key, keyBytes.length, 0, 0, lenPtr);
  return { code, len: u32(lenPtr) };
}

function has(id, keyBytes) {
  const key = scratch(keyBytes);
  return wasm.__blaze_cache_has(id, key, keyBytes.length);
}

function del(id, keyBytes) {
  const key = scratch(keyBytes);
  return wasm.__blaze_cache_delete(id, key, keyBytes.length);
}

/** Decode the 56-byte stats struct into a plain object of BigInts. */
function stats(id) {
  const buf = alloc(56);
  const code = wasm.__blaze_cache_stats(id, buf, 56);
  assert.equal(code, OK, `stats must succeed, got ${code}`);
  const out = {};
  STATS_FIELDS.forEach((name, i) => {
    out[name] = u64(buf + i * 8);
  });
  return out;
}

function concat(parts) {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const part of parts) {
    out.set(part, off);
    off += part.length;
  }
  return out;
}

const log = (step, detail) => console.log(`  ${step.padEnd(28)} ${detail}`);

// ── the round-trip ────────────────────────────────────────────────────────────
console.log(`harness: ${path}`);

// 0. the clock export exists and wasm starts frozen (no clock of its own — ADR-0002).
wasm.__blaze_tick(1_000);

// 1. open.
const id = open(8);
assert.ok(id > 0, `open must return an id, got ${id}`);
log('open', `id=${id} capacity=8`);

// 2. set a Ref handle and a 256-byte value covering every possible byte.
const REF_HANDLE = 0xdead_beef;
const REF_KEY = new TextEncoder().encode('ref-key');
const BLOB_KEY = new TextEncoder().encode('blob');
const BLOB = Uint8Array.from({ length: 256 }, (_, i) => i);

assert.equal(set(id, REF_KEY, { kind: KIND_REF, ref: REF_HANDLE }), OK, 'set a Ref');
assert.equal(
  set(id, BLOB_KEY, { kind: KIND_BYTES, value: BLOB, tags: ['binary', 'demo'] }),
  OK,
  'set bytes with tags',
);
log('set', `Ref(0x${REF_HANDLE.toString(16)}) + Bytes[${BLOB.length}] + 2 tags`);

// 3. get_sync: probe first (sizes the buffer), then a byte-for-byte read-back.
const refProbe = probe(id, REF_KEY);
assert.equal(refProbe.code, OK, 'probe the Ref key');
assert.equal(refProbe.len, 4, 'a Ref value is always 4 bytes');

const refGet = get(id, REF_KEY, 4);
assert.equal(refGet.code, OK, 'get the Ref value');
assert.deepEqual(
  [...refGet.bytes],
  [REF_HANDLE & 0xff, (REF_HANDLE >>> 8) & 0xff, (REF_HANDLE >>> 16) & 0xff, (REF_HANDLE >>> 24) & 0xff],
  'the Ref value round-trips as 4 little-endian bytes',
);
log('get_sync Ref', `len=${refGet.len} bytes=[${[...refGet.bytes].map((b) => b.toString(16).padStart(2, '0')).join(' ')}]`);

const blobProbe = probe(id, BLOB_KEY);
assert.equal(blobProbe.code, OK, 'probe the bytes key');
assert.equal(blobProbe.len, BLOB.length, 'the probe reports the stored length');

const blobGet = get(id, BLOB_KEY, BLOB.length);
assert.equal(blobGet.code, OK, 'get the bytes value');
assert.equal(blobGet.bytes.length, BLOB.length, 'the full value fits');
assert.deepEqual([...blobGet.bytes], [...BLOB], 'every one of the 256 byte values round-trips');
log('get_sync Bytes', `len=${blobGet.len}, byte-for-byte round-trip OK`);

// A too-small out buffer is BAD_ARG (the host resizes and retries) — and the probe
// above is exactly how it learns the size.
const tooSmall = get(id, BLOB_KEY, 8);
assert.equal(tooSmall.code, -2, 'an out buffer smaller than the value is BAD_ARG');
log('get_sync too small', 'BAD_ARG as pinned');

// 4. presence.
assert.equal(has(id, REF_KEY), OK, 'has: stored key');
assert.equal(has(id, new TextEncoder().encode('absent')), MISS, 'has: missing key');
log('has', 'present=OK, absent=MISS');

// 5. stats: the seven pinned fields, decoded from linear memory.
//    hits = 2 probes + 2 read-backs + the too-small get's lookup (the engine counts the
//    hit before the write fails); misses = 0 because `has` is a pure probe.
let s = stats(id);
log('stats', STATS_FIELDS.map((f) => `${f}=${s[f]}`).join(' '));
assert.equal(s.capacity, 8n, 'capacity is the configured entry count');
assert.equal(s.size, 2n, 'two live entries');
assert.equal(s.hits, 5n, 'four successful lookups + the too-small get');
assert.equal(s.misses, 0n, 'has is a pure probe and nothing has missed yet');
assert.equal(s.evictions, 0n, 'capacity was never exceeded');
assert.equal(s.expirations, 0n, 'nothing has expired yet');

// 6. TTL expiry, driven entirely by the host clock: tick before every time-dependent
// call (wasm has no clock of its own).
const TTL_KEY = new TextEncoder().encode('ttl-key');
wasm.__blaze_tick(1_000); // t = 1000 ms
assert.equal(set(id, TTL_KEY, { kind: KIND_REF, ref: 7, ttlMs: 500n }), OK, 'set with ttl=500ms');
wasm.__blaze_tick(1_600); // t = 1600 ms > 1500 ms deadline
const expiredGet = probe(id, TTL_KEY);
assert.equal(expiredGet.code, MISS, 'the entry is gone after its deadline');
assert.equal(has(id, TTL_KEY), MISS, 'has agrees: expired');
log('TTL expiry', 'tick(1000) → set(ttl=500) → tick(1600) → MISS');

// The lazy expiry reclaimed it: size is back to two, expirations counted once.
s = stats(id);
assert.equal(s.size, 2n, 'the expired entry was reclaimed');
assert.equal(s.expirations, 1n, 'exactly one expiration');
assert.equal(s.hits, 5n, 'the expired probe counted as a miss, not a hit');
assert.equal(s.misses, 1n, 'only the expired probe counted as a miss');
log('stats after TTL', STATS_FIELDS.map((f) => `${f}=${s[f]}`).join(' '));

// 7. delete, then everything on the id is gone.
assert.equal(del(id, REF_KEY), OK, 'delete a stored key');
assert.equal(del(id, REF_KEY), MISS, 'delete again is a miss');
assert.equal(probe(id, REF_KEY).code, MISS, 'the key is gone');
log('delete', 'stored=OK, again=MISS, gone=MISS');

s = stats(id);
log('stats after delete', STATS_FIELDS.map((f) => `${f}=${s[f]}`).join(' '));
assert.equal(s.size, 1n, 'only the blob entry is left');
assert.equal(s.invalidations, 1n, 'the successful delete counted one invalidation');
assert.ok(s.misses >= 2n, `the misses so far are counted, got ${s.misses}`);

// 8. the generation export is readable from JS and moves on every mutation.
const genBefore = wasm.__blaze_generation(id);
assert.equal(typeof genBefore, 'bigint', 'generation is a u64 (BigInt in JS)');
assert.ok(genBefore > 1n, `generation must have moved past 1, got ${genBefore}`);
assert.equal(wasm.__blaze_generation(9999), 0n, 'an unknown id reports 0');
log('generation', `id=${genBefore}, unknown=0`);

// 9. close consumes the id.
assert.equal(wasm.__blaze_cache_close(id), OK, 'close');
assert.equal(wasm.__blaze_cache_close(id), CLOSED, 'double close is CLOSED');
assert.equal(set(id, REF_KEY, { kind: KIND_REF, ref: 1 }), CLOSED, 'set on a closed id');
assert.equal(probe(id, REF_KEY).code, CLOSED, 'get on a closed id');
assert.equal(wasm.__blaze_generation(id), genBefore + 1n, 'close bumped the generation');
log('close', `CLOSED after close; generation=${wasm.__blaze_generation(id)}`);

console.log('harness: PASS — Ref + bytes round-tripped with no generated glue');
