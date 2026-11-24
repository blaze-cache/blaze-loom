/** Component Model arm: WIT guest, wasm-tools componentization, jco-transpiled JS host. */
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, 'component.wasm');
const jsUrl = pathToFileURL(join(here, 'out', 'spike.js')).href;
let seq = 0;

function wrap(m) {
  return {
    set: (k, v) => m.store.set(k, v),
    has: (k) => m.store.has(k),
    get: (k) => m.store.getSync(k) ?? null,
  };
}

export async function load() {
  const wasmBytes = await readFile(wasmPath);
  return { name: 'Component Model / WIT (jco)', api: wrap(await import(jsUrl)), wasmBytes };
}

// A fresh import re-evaluates the transpiled runtime and instantiates the core module, which is
// what a first page load pays for this arm.
export async function measureCold() {
  await import(`${jsUrl}?cold=${seq++}`);
}

export function create() {
  throw new Error('use load(); the jco runtime is instantiated by module import');
}
