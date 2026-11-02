#!/usr/bin/env node
// Compares two dumps of the dev gallery (see src/lib/dev/gallery and scripts/gallery-dump-plugin.mjs).
//
//   node scripts/gallery-diff.mjs <before-dir> <after-dir> [--strict] [--only=<substring>] [--context=<chars>]
//
// For every scene present in both dumps it reports whether the rendered DOM changed. Svelte puts a
// hash of the component's CSS in every class (`svelte-1x2y3z`), so a restyle changes the hash while
// the markup stays the same: those are reported separately ("style only"). With --strict the hash
// is kept in the comparison, i.e. a style-only change counts as a change.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const args = process.argv.slice(2);
const [beforeDir, afterDir] = args.filter((a) => !a.startsWith('--'));
const flag = (name, fallback) => args.find((a) => a.startsWith(`--${name}=`))?.split('=')[1] ?? fallback;
const strict = args.includes('--strict');
const only = flag('only', '');
const ctx = Number(flag('context', 90));

if (!beforeDir || !afterDir) {
  console.error('usage: node scripts/gallery-diff.mjs <before-dir> <after-dir> [--strict] [--only=x] [--context=n]');
  process.exit(2);
}

const load = (dir) => {
  const out = new Map();
  for (const f of readdirSync(dir).filter((n) => n.endsWith('.json'))) {
    const r = JSON.parse(readFileSync(join(dir, f), 'utf8'));
    out.set(r.id, r);
  }
  return out;
};

// Hashes and per-instance ids are noise for "did the structure change".
const normalise = (html) =>
  (html ?? '')
    // Svelte's anchor comments and the whitespace between tags do not render.
    .replace(/<!--.*?-->/g, '')
    .replace(/>\s+</g, '><')
    // Fixtures stamp "now" into times and dates, so two runs minutes apart never match.
    .replace(/\b\d{1,2}:\d{2}(:\d{2})?\b/g, 'T')
    .replace(/\b\d{2}\/\d{2}\/\d{4}\b|\b\d{4}-\d{2}-\d{2}\b/g, 'D')
    // Vite's own error overlay is dev tooling, not the app.
    .replace(/<vite-error-overlay><\/vite-error-overlay>/g, '')
    .replace(/(svelte-|s-)[A-Za-z0-9_-]{6,}/g, 's-H')
    .replace(/\b(id|for|aria-labelledby|aria-controls|aria-describedby)="[^"]*\d{3,}[^"]*"/g, '$1="N"');

/** First and last byte at which two strings differ, as short excerpts around the change. */
function firstChange(a, b) {
  let i = 0;
  const max = Math.min(a.length, b.length);
  while (i < max && a[i] === b[i]) i++;
  let ea = a.length;
  let eb = b.length;
  while (ea > i && eb > i && a[ea - 1] === b[eb - 1]) { ea--; eb--; }
  const cut = (s, from, to) => s.slice(Math.max(0, from - ctx), Math.min(s.length, to + ctx));
  return { before: cut(a, i, ea), after: cut(b, i, eb), at: i };
}

const before = load(beforeDir);
const after = load(afterDir);
const report = { structure: [], styleOnly: [], status: [], same: 0, onlyBefore: [], onlyAfter: [] };

for (const [id, b] of before) {
  if (only && !id.includes(only)) continue;
  const a = after.get(id);
  if (!a) { report.onlyBefore.push(id); continue; }
  if (b.state !== a.state) { report.status.push(`${id}: ${b.state} → ${a.state}${a.error ? ` (${a.error.split('\n')[0]})` : ''}`); continue; }
  if (b.html === a.html) { report.same++; continue; }
  const nb = normalise(b.html);
  const na = normalise(a.html);
  if (nb !== na) report.structure.push({ id, ...firstChange(nb, na) });
  else if (strict) report.styleOnly.push({ id, ...firstChange(b.html, a.html) });
  else report.styleOnly.push({ id });
}
for (const id of after.keys()) if (!before.has(id) && (!only || id.includes(only))) report.onlyAfter.push(id);

const line = (s) => console.log(s);
line(`scenes compared: ${before.size} before / ${after.size} after — identical: ${report.same}`);
line(`structure changed: ${report.structure.length}   style only: ${report.styleOnly.length}   status changed: ${report.status.length}`);
if (report.onlyBefore.length) line(`removed: ${report.onlyBefore.join(', ')}`);
if (report.onlyAfter.length) line(`added: ${report.onlyAfter.join(', ')}`);
if (report.status.length) { line('\n== status changed =='); report.status.forEach((s) => line(`  ${s}`)); }
if (report.structure.length) {
  line('\n== structure changed ==');
  for (const c of report.structure) {
    line(`\n  ${c.id}`);
    line(`    before: …${c.before}…`);
    line(`    after:  …${c.after}…`);
  }
}
if (report.styleOnly.length) {
  line('\n== style only (CSS hash changed, markup same) ==');
  line(`  ${report.styleOnly.map((c) => c.id).join(', ')}`);
}
