/**
 * M3 benchmark harness. Each arm exports `load()` -> { name, api, wasmBytes } and
 * `create(wasmBytes)` -> api. All arms run the identical workload:
 *   - binary size: .wasm bytes + on-disk JS glue bytes
 *   - cold start: median of 50 fresh Module+Instance+bind cycles
 *   - call cost: median of 5 x 20,000 get_sync() calls on a 16 B value
 *   - 1 KB set: median of 5 x 2,000 set() calls with a 1,024 B value
 * Run: node spike/m3/harness.mjs [arm ...]
 */
import { readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { dirname, extname, join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const known = ['flat-raw', 'loom', 'wasm-bindgen', 'component'];
const arms = process.argv.slice(2).length ? process.argv.slice(2) : known;

const encoder = new TextEncoder();
const key = encoder.encode('m3-hot-key');
const smallValue = encoder.encode('0123456789abcdef');
const kbValue = new Uint8Array(1024).fill(0x5a);

function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

async function glueBytes(dir) {
  let total = 0;
  async function walk(d) {
    for (const e of await readdir(d, { withFileTypes: true })) {
      if (e.name === 'node_modules' || e.name === 'target' || e.name === 'generate.mjs') continue;
      const p = join(d, e.name);
      if (e.isDirectory()) await walk(p);
      else if (['.js', '.mjs', '.cjs'].includes(extname(e.name))) {
        total += (await stat(p)).size;
      }
    }
  }
  await walk(dir);
  return total;
}

const results = [];
for (const arm of arms) {
  const dir = join(here, arm);
  let mod;
  try {
    mod = await import(pathToFileURL(join(dir, 'host.mjs')).href);
  } catch (err) {
    console.error(`SKIP ${arm}: ${err.message}`);
    continue;
  }

  const { name, api, wasmBytes } = await mod.load();
  api.set(key, smallValue);
  for (let i = 0; i < 2_000; i++) {
    api.get(key);
    api.set(key, smallValue);
  }

  // Cold start = compile + instantiate the raw module. Glue initialization is a one-off at
  // import time; its cost is reported separately as glue_bytes. Arms with non-core wasm can
  // export measureCold() to override this.
  const cold = [];
  for (let i = 0; i < 50; i++) {
    const t = performance.now();
    if (mod.measureCold) await mod.measureCold(wasmBytes);
    else new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), mod.imports ?? {});
    cold.push(performance.now() - t);
  }

  const callTrials = [];
  for (let trial = 0; trial < 5; trial++) {
    const t = performance.now();
    for (let i = 0; i < 20_000; i++) api.get(key);
    callTrials.push(performance.now() - t);
  }

  const setTrials = [];
  for (let trial = 0; trial < 5; trial++) {
    const t = performance.now();
    for (let i = 0; i < 2_000; i++) api.set(key, kbValue);
    setTrials.push(performance.now() - t);
  }

  results.push({
    arm,
    name,
    wasm_bytes: wasmBytes.length,
    glue_bytes: await glueBytes(dir),
    cold_start_ms: Number(median(cold).toFixed(4)),
    call_us: Number((median(callTrials) / 20_000 * 1000).toFixed(4)),
    set_1kb_us: Number((median(setTrials) / 2_000 * 1000).toFixed(4)),
  });
}

await writeFile(join(here, 'results.json'), `${JSON.stringify(results, null, 2)}\n`);
console.log('| arm | wasm B | glue B | cold ms | get_sync us | set 1KB us |');
console.log('|---|---:|---:|---:|---:|---:|');
for (const r of results) {
  console.log(`| ${r.name} | ${r.wasm_bytes} | ${r.glue_bytes} | ${r.cold_start_ms} | ${r.call_us} | ${r.set_1kb_us} |`);
}
