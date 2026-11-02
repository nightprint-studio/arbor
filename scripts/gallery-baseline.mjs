#!/usr/bin/env node
// Prepares a copy of the app at another git revision, with the CURRENT dev gallery laid on top,
// so the same scenes can be rendered on both and compared with scripts/gallery-diff.mjs.
//
//   node scripts/gallery-baseline.mjs [ref] [out-dir]
//
// `ref` defaults to HEAD, `out-dir` to .gallery-baseline (git-ignored). The copy is exported with
// `git archive` (no worktree, no checkout: the working tree and .git are left alone) and shares
// this checkout's node_modules through a symlink. The gallery files are always taken from the
// working tree, so a revision that predates them still gets scenes, mock and fixtures.
import { cpSync, existsSync, mkdirSync, rmSync, symlinkSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const ref = process.argv[2] ?? 'HEAD';
const out = resolve(process.argv[3] ?? join(root, '.gallery-baseline'));

// What `vite dev` needs from the revision; everything else (Rust, assets, plugins) is irrelevant.
const EXPORT = ['src', 'static', 'svelte.config.js', 'tsconfig.json', 'vite.config.js', 'package.json'];
// The gallery itself, always from the working tree.
const OVERLAY = [
  'src/lib/dev',
  'src/routes/dev-gallery',
  'src/routes/+layout.ts',
  'vite.gallery.config.js',
  'scripts/gallery-dump-plugin.mjs',
];

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const archive = execFileSync('git', ['archive', ref, ...EXPORT], { cwd: root, maxBuffer: 1 << 30 });
execFileSync('tar', ['-x', '-C', out], { input: archive });

for (const rel of OVERLAY) {
  const dest = join(out, rel);
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(join(dest, '..'), { recursive: true });
  cpSync(join(root, rel), dest, { recursive: true });
}
if (!existsSync(join(out, 'node_modules'))) symlinkSync(join(root, 'node_modules'), join(out, 'node_modules'));

console.log(`baseline of ${ref} ready in ${out}\n`);
console.log('Start it (port 1421) with its own dependency cache and snapshot folder:\n');
console.log(`  cd ${out}`);
console.log(`  GALLERY_CACHE=${root}/node_modules/.vite-gallery-baseline GALLERY_OUT=${out}/dump npx vite dev --config vite.gallery.config.js --port 1421\n`);
console.log('and the working tree on 1420:\n');
console.log(`  GALLERY_OUT=${root}/.gallery-out yarn gallery\n`);
console.log('Open /dev-gallery?mock on each at the SAME viewport size, run `await window.__gallery.dumpAll()`, then:\n');
console.log(`  yarn gallery:diff ${out}/dump ${root}/.gallery-out`);
