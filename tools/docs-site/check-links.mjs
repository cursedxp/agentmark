// Checks that every in-page link (href="#...") in the built docs page points at an existing id.
// Usage: node check-links.mjs [path-to-html]   (default: dist/agentmark-docs.html)
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const file = resolve(process.argv[2] ?? join(here, 'dist/agentmark-docs.html'));
const html = readFileSync(file, 'utf8');

const ids = new Set([...html.matchAll(/\sid="([^"]*)"/g)].map((m) => m[1]));
const decode = (s) => {
  try { return decodeURIComponent(s); } catch { return s; }
};
const missing = new Map();
for (const m of html.matchAll(/href="#([^"]*)"/g)) {
  const target = decode(m[1]);
  if (target === '' || ids.has(target)) continue;
  missing.set(target, (missing.get(target) ?? 0) + 1);
}
if (missing.size) {
  console.error(`broken in-page links in ${file}:`);
  for (const [t, n] of missing) console.error(`  #${t} (${n}x)`);
  process.exit(1);
}
console.log(`link check ok: ${ids.size} ids, all href="#..." targets exist.`);
