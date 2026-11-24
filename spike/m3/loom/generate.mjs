/**
 * Minimal build-time Loom generator: parse raw export signatures from spike.loom and emit
 * plain JS bindings. No runtime dependency, no Rust glue. Run from this directory:
 *   node generate.mjs
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const schema = await readFile(join(here, 'spike.loom'), 'utf8');

/** @type {{ name: string, args: string[], result: string }[]} */
const functions = [];
for (const line of schema.split('\n')) {
  const m = line.match(/^\s*(?:export|fn)\s+(\w+)\s*\(([^)]*)\)\s*->\s*(\w+)/);
  if (!m) continue;
  const args = m[2]
    .split(',')
    .map((a) => a.trim())
    .filter(Boolean)
    .map((a) => a.split(':')[0].trim());
  functions.push({ name: m[1], args, result: m[3] });
}
if (functions.length === 0) throw new Error('no function signatures found in spike.loom');

const wrappers = functions
  .map((f) => `    ${f.name}: (${f.args.join(', ')}) => e.${f.name}(${f.args.join(', ')}),`)
  .join('\n');
const checks = functions.map((f) => `  if (typeof e.${f.name} !== 'function') throw new Error('missing export: ${f.name}');`).join('\n');

const output = `// Generated from spike.loom by generate.mjs — do not edit.
const EXPORTS = ${JSON.stringify(functions.map((f) => f.name))};

export function bind(instance) {
  const e = instance.exports;
${checks}
  return {
${wrappers}
  };
}
`;

await mkdir(join(here, 'generated'), { recursive: true });
await writeFile(join(here, 'generated', 'bindings.mjs'), output);
console.log(`generated ${functions.length} bindings from spike.loom`);
