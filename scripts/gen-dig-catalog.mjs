/**
 * Generate Bennu's `.dig` language catalog from **geode's own** translation files.
 *
 *   node scripts/gen-dig-catalog.mjs --geode ../../games/geode [--lang en]
 *
 * `--lang` defaults to **en**: all of Arbor is in English, and this text is read inside
 * Arbor's editor, not inside the game. geode declares `it` first (it is an Italian game
 * and starts in Italian) — that is the right default *there* and the wrong one here, so
 * the two do not share it. Both locales are complete; a geode test requires a line per
 * vocabulary word in every declared language, which is why switching is just this flag.
 *
 * The `.dig` vocabulary is geode's, and it is already written down there — the names in
 * `nd_schema::vocabulary::VOCABULARY` and the help text in
 * `content/core/i18n/<lang>/*.toml`, with a geode test pinning one to the other (every
 * word of the vocabulary must have a line in every declared language). So the keys of
 * `lang.toml` **are** the vocabulary, and this script needs no second list.
 *
 * ## Why a generated file and not a read at runtime
 *
 * Reading the `.toml` from the opened project would keep the two in lockstep forever,
 * and it was the first design. Two things ruled it out: parsing TOML in the WebView
 * needs a dependency Arbor does not have (hard rule 7), and it would tie `.dig` support
 * to having *that* project open — a `.dig` file opened on its own would lose its
 * completion. So the catalog is copied, and the copy is **generated** rather than
 * hand-transcribed: when geode's vocabulary moves, this is one command, not an
 * archaeology session across 49 entries.
 *
 * ⚠️ The copy WILL age. That is the accepted cost of the decision; re-run this after a
 * geode change that adds or rewords a builtin. The generated header records which geode
 * checkout and language it came from so the drift is visible.
 *
 * Emits a `.ts` module (not JSON) so the shape is typed at the call site and no
 * `resolveJsonModule` / asset-loading question arises.
 *
 * `crystals.toml` is deliberately not read: its entries are *templates* with `{name}` /
 * `{tool}` placeholders that geode fills from the loaded `.ron` catalogue at runtime.
 * The `[Crystal]` members of `symbols.toml` are the same information already rendered,
 * and they are what geode's own editor falls back to without a live catalogue — which is
 * exactly Bennu's situation.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const OUT = join(REPO, 'src/lib/components/bennu/dig/catalog.ts');

// ── args ──────────────────────────────────────────────────────────────────────

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

const geode = resolve(REPO, arg('geode', '../../games/geode'));
const lang = arg('lang', 'en');
const i18n = join(geode, 'content/core/i18n', lang);

// ── a minimal TOML reader, for the shape these files actually are ─────────────
//
// Comment lines, `[Section]` headers, and `key = "basic string"` on one line. That is
// the whole grammar of geode's i18n files (a test over there enforces the format), so a
// real parser would be the heavier half of this script. Anything else is skipped rather
// than guessed at.

/** Unescape a TOML basic string's body (`\n`, `\t`, `\"`, `\\`, `\uXXXX`). */
function unescape(body) {
  return body.replace(/\\(u[0-9a-fA-F]{4}|.)/g, (_, esc) => {
    if (esc[0] === 'u') return String.fromCharCode(parseInt(esc.slice(1), 16));
    return { n: '\n', t: '\t', r: '\r', '"': '"', '\\': '\\' }[esc] ?? esc;
  });
}

/**
 * Parse into `{ '': { key: value }, Section: { key: value } }` — the top-level table
 * under the empty-string key, so a flat file and a sectioned one read the same way.
 */
function parseToml(text) {
  const out = { '': {} };
  let section = '';
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;

    const header = /^\[([^\]]+)\]$/.exec(line);
    if (header) {
      section = header[1].trim();
      out[section] ??= {};
      continue;
    }

    const assign = /^([A-Za-z0-9_.-]+)\s*=\s*"((?:[^"\\]|\\.)*)"\s*$/.exec(line);
    if (assign) out[section][assign[1]] = unescape(assign[2]);
  }
  return out;
}

function readToml(file) {
  const path = join(i18n, file);
  try {
    return parseToml(readFileSync(path, 'utf8'));
  } catch (err) {
    console.error(`cannot read ${path}: ${err.message}`);
    console.error(`(is --geode right? tried ${geode})`);
    process.exit(1);
  }
}

// ── read the four files ───────────────────────────────────────────────────────

const builtins = readToml('lang.toml')[''];
const keywords = readToml('keywords.toml')[''];
const symbolTables = readToml('symbols.toml');
const methodTables = readToml('methods.toml');

/** `[Namespace]` → `{ about, members }`; `about` is the namespace's own description and
 *  is reserved (geode has a test forbidding a member of that name). */
const namespaces = {};
for (const [name, table] of Object.entries(symbolTables)) {
  if (name === '') continue; // the file's preamble has no top-level keys anyway
  const { about = '', ...members } = table;
  namespaces[name] = { about, members };
}

const methods = {};
for (const kind of ['list', 'map']) {
  if (methodTables[kind]) methods[kind] = methodTables[kind];
}

// ── emit ──────────────────────────────────────────────────────────────────────

const counts = {
  builtins: Object.keys(builtins).length,
  keywords: Object.keys(keywords).length,
  namespaces: Object.keys(namespaces).length,
  members: Object.values(namespaces).reduce((n, ns) => n + Object.keys(ns.members).length, 0),
  methods: Object.values(methods).reduce((n, t) => n + Object.keys(t).length, 0),
};

for (const [what, n] of Object.entries(counts)) {
  if (n === 0) {
    console.error(`refusing to write an empty catalog: no ${what} found in ${i18n}`);
    process.exit(1);
  }
}

const body = {
  language: lang,
  builtins,
  keywords,
  namespaces,
  methods,
};

const source = `/**
 * The \`.dig\` language catalog — GENERATED, do not edit by hand.
 *
 *   node scripts/gen-dig-catalog.mjs --geode <path-to-geode> --lang ${lang}
 *
 * Copied from geode's own translation files (\`content/core/i18n/${lang}/{lang,keywords,symbols,methods}.toml\`),
 * which are the authoritative help text for the language: the keys of \`lang.toml\` are
 * the vocabulary itself (a geode test pins the two together), and the first line of each
 * entry is its **signature** — that formatting is a contract over there, and
 * \`dig-catalog.ts\` relies on it to split "signature" from "explanation".
 *
 * Re-run the generator after a geode change that adds or rewords a builtin.
 *
 * Contents: ${counts.builtins} builtins · ${counts.keywords} keywords ·
 * ${counts.namespaces} namespaces (${counts.members} members) · ${counts.methods} collection methods.
 */

import type { DigCatalog } from './dig-catalog';

export const DIG_CATALOG: DigCatalog = ${JSON.stringify(body, null, 2)};
`;

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, source, 'utf8');

console.log(`wrote ${OUT}`);
console.log(
  `  ${counts.builtins} builtins, ${counts.keywords} keywords, ${counts.namespaces} namespaces ` +
    `(${counts.members} members), ${counts.methods} collection methods — from ${i18n}`,
);
