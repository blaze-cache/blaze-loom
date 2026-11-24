/** Generated-glue arm: wasm-bindgen's JS bindings. */
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import * as wbg from './pkg/spike_wasm_bindgen.js';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, 'pkg', 'spike_wasm_bindgen_bg.wasm');

// The generated module imports one JS shim; supply a no-op so raw instantiation can be measured.
export const imports = {
  './spike_wasm_bindgen_bg.js': {
    __wbindgen_init_externref_table: () => {},
  },
};

function api() {
  return {
    set: (k, v) => wbg.set(k, v),
    has: (k) => wbg.has(k),
    get: (k) => wbg.get_sync(k) ?? null,
  };
}

export async function load() {
  const wasmBytes = await readFile(wasmPath);
  wbg.initSync({ module: wasmBytes });
  return { name: 'generated glue (wasm-bindgen)', api: api(), wasmBytes };
}

export function create(wasmBytes) {
  wbg.initSync({ module: wasmBytes });
  return api();
}
