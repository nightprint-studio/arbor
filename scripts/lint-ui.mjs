#!/usr/bin/env node
// UI consistency lint, run as a RATCHET: it never asks to fix the legacy
// backlog, it only refuses to let it grow. Zero dependencies (Node built-ins).
//
//   node scripts/lint-ui.mjs                   check against the baseline
//   node scripts/lint-ui.mjs --list [rule]     print every violation (optionally for one rule)
//   node scripts/lint-ui.mjs --update-baseline rewrite scripts/lint-ui.baseline.json
//
// Exit codes: 0 ok, 1 regression (a file got worse / a new file violates), 2 usage error.
// See docs/ui-lint.md for how to add a rule.

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { join, relative, dirname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const SCAN_DIR = join(ROOT, 'src', 'lib');
const BASELINE = join(ROOT, 'scripts', 'lint-ui.baseline.json');

// ---------------------------------------------------------------------------
// Source masking. Rules must not fire on comments (docs explaining a removal
// are legitimate), so comments are blanked with spaces: offsets and line
// numbers of the remaining code are unchanged.
// ---------------------------------------------------------------------------

const blank = (s) => s.replace(/[^\n]/g, ' ');

function stripComments(src) {
  return src
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    // `//` only when it starts a comment: not after `:` (URLs) or inside a quoted string on the same line.
    .replace(/^([^'"`\n]*?)(?<![:\w])\/\/.*$/gm, (m, head) => head + blank(m.slice(head.length)));
}

/** Ranges [start,end) of <style> contents in a .svelte file (whole file for .css). */
function styleRanges(file, text) {
  if (file.endsWith('.css')) return [[0, text.length]];
  if (!file.endsWith('.svelte')) return [];
  const out = [];
  for (const m of text.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/g)) {
    const start = m.index + m[0].indexOf('>') + 1;
    out.push([start, start + m[1].length]);
  }
  return out;
}

const inRanges = (ranges, i) => ranges.some(([a, b]) => i >= a && i < b);
const lineOf = (text, i) => text.slice(0, i).split('\n').length;

// ---------------------------------------------------------------------------
// Path helpers and allowlists
// ---------------------------------------------------------------------------

const UI_DIR = 'src/lib/components/shared/ui/';
const isSharedUi = (rel) => rel.startsWith(UI_DIR);
// In-app documentation pages and tests quote code samples (Java, JS) that must not be linted as UI code.
const isDocOrTest = (rel) => /\/docs\/[^/]*\.svelte$|\.test\.ts$/.test(rel);

// Files where literal colours are the point: theme definitions, brand marks,
// chart palettes, syntax-highlighting themes, colour pickers and previews.
const HEX_ALLOW = [
  /^src\/lib\/themes\//,
  /^src\/lib\/components\/shared\/charts\//,
  /^src\/lib\/components\/shared\/ui\/code-editor\//,
  /\/(Brand\w*|\w*Brand\w*|ArborLogo|\w*Logo\w*)\.svelte$/,
  /\/ThemeEditorModal\.svelte$/,
  /\/\w*(Color|Colour)\w*\.(svelte|ts)$/,
  /^src\/lib\/components\/studio\//,
  /^src\/lib\/components\/merula\//,
];

// ---------------------------------------------------------------------------
// Rules. `check(ctx)` returns [{ index, ... }] offsets into ctx.code (comments
// blanked); the runner turns them into file:line. Keep rules precise: a noisy
// rule trains people to update the baseline blindly.
// ---------------------------------------------------------------------------

/** Run a global regex over `code`, optionally restricted to a region predicate. */
function matches(code, re, keep = () => true) {
  const out = [];
  for (const m of code.matchAll(re)) if (keep(m)) out.push({ index: m.index });
  return out;
}

const RULES = [
  {
    id: 'no-native-dialogs',
    ext: ['.svelte', '.ts'],
    description: 'Browser-native window.prompt/confirm/alert dialogs',
    message:
      'Use a real Arbor modal: shared/ConfirmModal.svelte for yes/no, shared/PromptModal.svelte or shared/Modal.svelte for input. Native dialogs break the look of the app.',
    skipPath: isDocOrTest,
    check({ code }) {
      const hits = matches(code, /\b(?:window|globalThis)\.(?:prompt|confirm|alert)\s*\(/g);
      // Bare `confirm(` is only a native call when the file never declares or
      // passes around its own `confirm` (function, prop, callback, ...).
      for (const name of ['prompt', 'confirm', 'alert']) {
        const nonCall = new RegExp(`(?<![.\\w$])${name}\\b(?!\\s*\\()`);
        const bare = new RegExp(`(?<![.\\w$])(?<!\\b(?:void|function|async|new)\\s+)${name}\\s*\\(`, 'g');
        const declared = new RegExp(`\\b(?:function|const|let|var)\\s+${name}\\b`).test(code);
        if (declared || nonCall.test(code)) continue;
        hits.push(...matches(code, bare));
      }
      return hits;
    },
  },
  {
    id: 'no-backdrop-filter',
    ext: ['.svelte', '.ts', '.css'],
    description: 'Active backdrop-filter declarations',
    message:
      'Remove it: backdrop-filter keeps the WebView2 compositor busy and throttles child-process I/O. Use an opaque or semi-opaque var(--bg-*) surface instead.',
    check: ({ code }) => matches(code, /(?:-webkit-)?backdrop-filter\s*:\s*(?!none\b)/g),
  },
  {
    id: 'no-local-storage-settings',
    ext: ['.svelte', '.ts'],
    description: "localStorage keys with the 'arbor:' prefix",
    message:
      'User preferences live on disk: global -> ~/.config/arbor/config.toml (typed section + get/set command + store loaded in AppShell), per-repo -> <repo>/.arbor/config.toml. localStorage is only for ephemeral UI state.',
    check({ code }) {
      if (!/localStorage/.test(code)) return [];
      // Only keys that reach localStorage: same line as a localStorage call, or a *KEY constant.
      // Custom-event names and contribution points share the 'arbor:' prefix and are fine.
      return matches(code, /['"`]arbor:(?!\/\/)[\w.:-]+/g, (m) => {
        const ls = code.lastIndexOf('\n', m.index) + 1;
        const le = code.indexOf('\n', m.index);
        const line = code.slice(ls, le < 0 ? code.length : le);
        return /localStorage/.test(line) || /\b(?:const|let)\s+\w*(?:KEY|Key|key)\w*\s*(?::[^=]+)?=/.test(line);
      });
    },
  },
  {
    id: 'no-adhoc-button',
    ext: ['.svelte'],
    description: '<button class=...> outside shared/ui',
    message:
      'Use shared/ui/Button.svelte (or SplitButton / SidebarItem / CopyButton ...). If no variant fits, extend the widget instead of hand-rolling a button.',
    skipPath: isSharedUi,
    check: ({ code }) => matches(code, /<button\b[^>]*?\bclass\s*=/g),
  },
  {
    id: 'no-native-checkbox',
    ext: ['.svelte'],
    description: 'type="checkbox" outside shared/ui',
    message: 'Use shared/ui/Toggle.svelte (or a checkbox widget in shared/ui) so focus, a11y and styling stay in one place.',
    skipPath: isSharedUi,
    check: ({ code }) => matches(code, /\btype\s*=\s*(?:["']|\{\s*["'])checkbox\b/g),
  },
  {
    id: 'no-native-select',
    ext: ['.svelte'],
    description: '<select> outside shared/ui',
    message: 'Use shared/ui/Select.svelte or Dropdown.svelte (filterable, keyboard-first, themed).',
    skipPath: (rel) => isSharedUi(rel) || isDocOrTest(rel),
    check: ({ code }) => matches(code, /<select\b/g),
  },
  {
    id: 'no-hex-color',
    ext: ['.svelte', '.css'],
    description: 'Literal hex / rgb() / rgba() colours in <style> blocks and style attributes',
    message:
      'Use a theme token (var(--accent), var(--bg-elevated), var(--danger) ...) or color-mix(in srgb, var(--token) N%, transparent) for alpha. Literal colours do not follow the theme.',
    skipPath: (rel) => HEX_ALLOW.some((re) => re.test(rel)),
    check({ file, code, styles }) {
      const colour = /#[0-9a-fA-F]{3,8}\b|\brgba?\s*\(/g;
      const hits = [];
      for (const m of code.matchAll(colour)) {
        const i = m.index;
        if (inRanges(styles, i)) {
          // Declarations only: skip selectors such as `#abc {` (ids that look like hex).
          const ls = code.lastIndexOf('\n', i) + 1;
          const le = code.indexOf('\n', i);
          const line = code.slice(ls, le < 0 ? code.length : le);
          if (/\{\s*$/.test(line) && !line.includes(':')) continue;
          if (m[0].startsWith('#') && !/:[^;]*$/.test(code.slice(ls, i))) continue;
          hits.push({ index: i });
        } else if (file.endsWith('.svelte')) {
          // style="..." / style={...} attributes in markup.
          const before = code.slice(Math.max(0, i - 400), i);
          if (/\bstyle(?::[\w-]+)?\s*=\s*(?:"[^"]*|'[^']*|\{[^}]*)$/.test(before)) hits.push({ index: i });
        }
      }
      return hits;
    },
  },
  {
    id: 'no-outline-none',
    ext: ['.svelte', '.css'],
    description: 'outline: none/0 with no visible focus replacement',
    message:
      'Never drop the focus ring silently: keep the outline, or pair `outline: none` with a `:focus-visible` rule (box-shadow / border-color) so keyboard users still see focus.',
    check({ code, styles }) {
      // A file that styles :focus-visible anywhere is trusted to provide the replacement;
      // a per-rule check would need a real CSS parser for little extra precision.
      if (/:focus-visible|:focus-within/.test(code)) return [];
      const out = [];
      for (const m of code.matchAll(/\boutline\s*:\s*(?:none|0)\b/g)) {
        if (!inRanges(styles, m.index)) continue;
        const open = code.lastIndexOf('{', m.index);
        const close = code.indexOf('}', m.index);
        const body = code.slice(open + 1, close < 0 ? code.length : close);
        if (/box-shadow\s*:\s*(?!none)|border(?:-color)?\s*:\s*(?!none|0\b)/.test(body)) continue;
        out.push({ index: m.index });
      }
      return out;
    },
  },
  {
    id: 'no-numeric-z-index',
    ext: ['.svelte', '.css'],
    description: 'Numeric z-index literals in CSS',
    message:
      'Use a stacking token (var(--z-dropdown), var(--z-modal), var(--z-toast) ...) so layers are ordered in one place. 0/1/-1 for local stacking inside a component are the only literals worth keeping.',
    check({ code, styles }) {
      return matches(code, /\bz-index\s*:\s*-?\d+/g, (m) => inRanges(styles, m.index));
    },
  },
  {
    id: 'no-global-in-ui',
    ext: ['.svelte', '.css'],
    description: ':global( inside shared/ui',
    message:
      'Widgets in shared/ui must be self-contained: take a class/prop from the consumer or wrap the target in the widget instead of reaching out with :global().',
    onlyPath: isSharedUi,
    check: ({ code }) => matches(code, /:global\(/g),
  },
  {
    id: 'no-ctrl-alt-shortcut',
    ext: ['.svelte', '.ts'],
    description: 'Ctrl+Alt+<letter> keybindings',
    skipPath: isDocOrTest,
    message:
      'Chromium drops Ctrl+Alt chords to preserve AltGr on IT/DE/FR/ES keyboards. Use Alt+Shift+<letter>, Ctrl+Shift+<letter> or an F-key.',
    check({ code }) {
      return [
        ...matches(code, /ctrl\s*:\s*true[^}\n]*\balt\s*:\s*true|\balt\s*:\s*true[^}\n]*\bctrl\s*:\s*true/g),
        ...matches(code, /['"`]Ctrl\+Alt\+[A-Za-z]\b/g),
      ];
    },
  },
];

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

function* walk(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (/\.(svelte|ts|css)$/.test(e.name) && !e.name.endsWith('.d.ts')) yield p;
  }
}

/** -> { [ruleId]: { [relFile]: [{line, text}] } } */
function scan() {
  const found = Object.fromEntries(RULES.map((r) => [r.id, {}]));
  for (const abs of walk(SCAN_DIR)) {
    const rel = relative(ROOT, abs).split(sep).join('/');
    // The dev gallery and the Tauri mock never ship (guarded by `dev`) and use literal colours
    // for their own chrome; linting them as app UI would only add noise.
    if (rel.startsWith('src/lib/dev/')) continue;
    const ext = rel.slice(rel.lastIndexOf('.'));
    const raw = readFileSync(abs, 'utf8');
    const code = stripComments(raw);
    const styles = styleRanges(rel, code);
    const rawLines = raw.split('\n');
    for (const rule of RULES) {
      if (!rule.ext.includes(ext)) continue;
      if (rule.skipPath?.(rel)) continue;
      if (rule.onlyPath && !rule.onlyPath(rel)) continue;
      const hits = rule.check({ file: rel, code, styles });
      if (!hits.length) continue;
      found[rule.id][rel] = hits
        .map((h) => lineOf(code, h.index))
        .sort((a, b) => a - b)
        .map((line) => ({ line, text: (rawLines[line - 1] ?? '').trim().slice(0, 110) }));
    }
  }
  return found;
}

const counts = (found) =>
  Object.fromEntries(
    Object.entries(found).map(([id, files]) => [
      id,
      Object.fromEntries(Object.entries(files).map(([f, v]) => [f, v.length]).sort(([a], [b]) => a.localeCompare(b))),
    ]),
  );

const total = (byFile) => Object.values(byFile).reduce((a, b) => a + b, 0);

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith('--')));
const positional = args.filter((a) => !a.startsWith('--'));
const known = new Set(['--list', '--update-baseline', '--help']);
for (const f of flags) {
  if (!known.has(f)) {
    console.error(`unknown option ${f} (use --list [rule], --update-baseline)`);
    process.exit(2);
  }
}
if (flags.has('--help')) {
  console.log('usage: node scripts/lint-ui.mjs [--list [rule-id]] [--update-baseline]');
  process.exit(0);
}

const found = scan();
const current = counts(found);

if (flags.has('--list')) {
  const only = positional[0];
  if (only && !RULES.some((r) => r.id === only)) {
    console.error(`unknown rule "${only}". Rules: ${RULES.map((r) => r.id).join(', ')}`);
    process.exit(2);
  }
  for (const rule of RULES.filter((r) => !only || r.id === only)) {
    const files = found[rule.id];
    console.log(`\n== ${rule.id} (${total(current[rule.id])}) - ${rule.description}`);
    for (const [f, hits] of Object.entries(files)) for (const h of hits) console.log(`  ${f}:${h.line}  ${h.text}`);
  }
  process.exit(0);
}

if (flags.has('--update-baseline')) {
  const out = {
    _comment:
      'Ratchet baseline for scripts/lint-ui.mjs: per-rule, per-file violation counts. Only ever regenerate it after counts went DOWN (node scripts/lint-ui.mjs --update-baseline).',
    rules: Object.fromEntries(Object.entries(current).filter(([, files]) => Object.keys(files).length)),
  };
  writeFileSync(BASELINE, JSON.stringify(out, null, 2) + '\n');
  console.log(`baseline written: ${relative(ROOT, BASELINE)}`);
  for (const r of RULES) console.log(`  ${r.id.padEnd(28)} ${String(total(current[r.id])).padStart(5)}`);
  process.exit(0);
}

if (!existsSync(BASELINE)) {
  console.error('missing scripts/lint-ui.baseline.json - run: node scripts/lint-ui.mjs --update-baseline');
  process.exit(2);
}
const base = JSON.parse(readFileSync(BASELINE, 'utf8')).rules ?? {};

let failed = false;
let improved = false;
for (const rule of RULES) {
  const baseFiles = base[rule.id] ?? {};
  const regressions = [];
  for (const [file, hits] of Object.entries(found[rule.id])) {
    const allowed = baseFiles[file] ?? 0;
    if (hits.length > allowed) regressions.push({ file, allowed, hits });
  }
  for (const [file, n] of Object.entries(baseFiles)) {
    if ((current[rule.id][file] ?? 0) < n) improved = true;
  }
  if (!regressions.length) continue;
  failed = true;
  console.error(`\nFAIL ${rule.id}: ${rule.description}`);
  console.error(`  ${rule.message}`);
  for (const { file, allowed, hits } of regressions) {
    console.error(`  ${file}: ${hits.length} violation(s), baseline ${allowed}`);
    // Lines can't be attributed to "new" ones after edits, so show them all for the file.
    for (const h of hits) console.error(`    ${file}:${h.line}  ${h.text}`);
  }
}

console.log('\nui-lint summary (current / baseline):');
for (const rule of RULES) {
  console.log(`  ${rule.id.padEnd(28)} ${String(total(current[rule.id])).padStart(5)} / ${String(total(base[rule.id] ?? {})).padStart(5)}`);
}

if (failed) {
  console.error('\nui-lint: FAILED - fix the new violations above (do not raise the baseline).');
  process.exit(1);
}
if (improved) {
  console.log('\nui-lint: counts went down. Lock in the progress: node scripts/lint-ui.mjs --update-baseline');
}
console.log('ui-lint: OK');
