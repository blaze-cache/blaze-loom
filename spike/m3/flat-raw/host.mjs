/** Hand-written JS decoder for the flat raw ABI. No generator involved. */
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, 'target', 'wasm32-unknown-unknown', 'release', 'spike_flat_raw.wasm');

export async function load() {
  const wasmBytes = await readFile(wasmPath);
  return { name: 'flat raw, hand-written', api: create(wasmBytes), wasmBytes };
}

export function create(wasmBytes) {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {});
  const e = instance.exports;
  const u8 = () => new Uint8Array(e.memory.buffer);

  function put(data) {
    const p = e.alloc(data.length);
    u8().set(data, p);
    return p;
  }

  return {
    set(k, v) {
      e.reset();
      e.set(put(k), k.length, put(v), v.length);
    },
    has(k) {
      e.reset();
      return e.has(put(k), k.length) === 1;
    },
    get(k, cap = 4096) {
      e.reset();
      const kp = put(k);
      const out = e.alloc(cap);
      const n = e.get_sync(kp, k.length, out, cap);
      return n < 0 ? null : u8().slice(out, out + n);
    },
  };
}
