/** Loom arm: schema-generated bindings plus a hand-written runtime. */
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bind } from './generated/bindings.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, 'target', 'wasm32-unknown-unknown', 'release', 'spike_loom.wasm');

export async function load() {
  const wasmBytes = await readFile(wasmPath);
  return { name: 'flat raw + Loom', api: create(wasmBytes), wasmBytes };
}

export function create(wasmBytes) {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {});
  const b = bind(instance);
  const u8 = () => new Uint8Array(instance.exports.memory.buffer);

  function put(data) {
    const p = b.alloc(data.length);
    u8().set(data, p);
    return p;
  }

  return {
    set(k, v) {
      b.reset();
      b.set(put(k), k.length, put(v), v.length);
    },
    has(k) {
      b.reset();
      return b.has(put(k), k.length) === 1;
    },
    get(k, cap = 4096) {
      b.reset();
      const kp = put(k);
      const out = b.alloc(cap);
      const n = b.get_sync(kp, k.length, out, cap);
      return n < 0 ? null : u8().slice(out, out + n);
    },
  };
}
