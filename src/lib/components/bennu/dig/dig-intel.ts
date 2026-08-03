/**
 * `.dig` completion + hover — resolved **locally**, from the generated catalog.
 *
 * Unlike Java (whose intelligence is a backend index) the `.dig` vocabulary is *closed*:
 * 49 builtins, 22 reserved words, five dotted namespaces, two collection method sets.
 * That makes completion a lookup rather than an inference, and there is nothing for
 * `bennu-be` to do — no RPC, no index, no waiting for a warm-up.
 *
 * ## Where the caret is decides what is offered
 *
 * Three contexts, read off the line up to the caret:
 *
 * 1. **After `Namespace.`** — that namespace's members, and only those. `Crystal.` never
 *    offers a keyword.
 * 2. **After any other `something.`** — the collection methods, both receivers, each
 *    labelled with the receiver it belongs to (`lista.append`, `mappa.has`). Bennu has
 *    no type inference for `.dig`, so it cannot know whether the receiver is a list or a
 *    map; showing both labelled says exactly that, where picking one would be a guess
 *    the reader could not see.
 * 3. **A bare word** — keywords, builtins, namespace names, plus the `fn` / `struct` /
 *    `let` names declared in this buffer.
 *
 * An `import` line is deliberately **not** completed. In geode a module is a library the
 * player unlocks in the shop (`from geo import path_to`), not a file on disk, and Bennu
 * has no way to know which are unlocked — offering the sibling `.dig` files would look
 * like an answer and be a wrong one.
 *
 * ## Local declarations come from a lexical scan, not a parse
 *
 * A `CompletionSource` receives the `EditorState`, not the live syntax tree (the tree
 * belongs to the highlight plugin), so the buffer's own names are found by scanning for
 * `fn` / `struct` / `let` at line starts. Two honest consequences: a name inside a
 * string or comment could slip in, and every `let` in the file is offered rather than
 * only those in scope — a **superset**, so nothing offered fails to exist. geode's own
 * in-game editor scopes them properly because it holds the AST; matching that here needs
 * the tree threaded into the completion seam, which is a change to the shared core.
 */

import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';
import type { EditorView, Tooltip } from '@codemirror/view';
import type { CodeEditorIntel } from '$lib/components/shared/ui/code-editor';
import { hoverCardDom } from '../bennu-hover';
import { DIG_CATALOG } from './catalog';
import { lookupMember, lookupMethod, lookupWord, splitEntry } from './dig-catalog';

/** Word characters in `.dig` — the grammar's `identifier` is `[A-Za-z_][A-Za-z0-9_]*`. */
const WORD = /[A-Za-z0-9_]/;

/** What CodeMirror may keep filtering against without re-querying: still one identifier.
 *  Anchored at both ends — it is tested against the *whole* completion range. */
const VALID_FOR = /^[A-Za-z0-9_]*$/;

/** The identifier being typed, for `matchBefore`. Anchored at the **end only**: that call
 *  searches within the text before the caret, so a leading `^` would demand the entire line
 *  be one identifier and never match on `let x = se`. */
const WORD_BEFORE = /[A-Za-z0-9_]*$/;

/** `object.prefix` immediately left of the caret. The object is a single identifier, not
 *  a chain: only a namespace (`Tool.`) or a variable (`trovati.`) can be completed, and
 *  `a.b.` is neither. */
const DOTTED = /([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z0-9_]*)$/;

/** A top-level declaration: `fn name(`, `struct name:`, `let name =`. Anchored at the
 *  line start (after indentation) so a `let` inside a string is much less likely to
 *  match, and the receiver of a method definition inside a `struct` still is. */
const DECLARATION = /^[ \t]*(fn|struct|let)[ \t]+([A-Za-z_][A-Za-z0-9_]*)/;

// ── Candidate builders ────────────────────────────────────────────────────────

/** Completion `type`s, chosen so the popup's icons read the way the language does. */
const KIND = {
  keyword: 'keyword',
  builtin: 'function',
  namespace: 'class',
  member: 'constant',
  method: 'method',
  fn: 'function',
  struct: 'class',
  variable: 'variable',
} as const;

/** One catalog entry → a completion whose `detail` is its signature and whose `info` is
 *  the explanation. The split is the format contract geode's `.toml` files keep (first
 *  line = the form); `info` gets the rest, so the popup teaches without a hover. */
function fromEntry(label: string, doc: string, type: string, boost = 0): Completion {
  const { signature } = splitEntry(doc);
  const rest = doc.slice(signature.length).trim();
  return { label, type, detail: signature, info: rest || undefined, boost };
}

/** The whole static vocabulary, built once: it never changes at runtime. */
const STATIC_OPTIONS: Completion[] = [
  // Builtins first: in a language this small they are what one is reaching for.
  ...Object.entries(DIG_CATALOG.builtins).map(([n, d]) => fromEntry(n, d, KIND.builtin, 1)),
  ...Object.entries(DIG_CATALOG.keywords).map(([n, d]) => fromEntry(n, d, KIND.keyword)),
  ...Object.entries(DIG_CATALOG.namespaces).map(([n, ns]) =>
    fromEntry(n, ns.about, KIND.namespace),
  ),
];

/** A namespace's members (`Tool.` → `Pick` / `Drill` / `Laser`). */
const MEMBER_OPTIONS: Record<string, Completion[]> = Object.fromEntries(
  Object.entries(DIG_CATALOG.namespaces).map(([name, ns]) => [
    name,
    Object.entries(ns.members).map(([m, d]) => fromEntry(m, d, KIND.member)),
  ]),
);

/**
 * The collection methods, both receivers, each labelled with the receiver in its detail.
 *
 * `has` exists on both, so the two entries share a label — CodeMirror shows both rows,
 * and their `detail` (`lista.has(x)` vs `mappa.has(chiave)`) is what tells them apart.
 * That is the intended reading: without inference, both are true.
 */
const METHOD_OPTIONS: Completion[] = Object.entries(DIG_CATALOG.methods).flatMap(
  ([, table]) => Object.entries(table).map(([m, d]) => fromEntry(m, d, KIND.method)),
);

/** The `fn` / `struct` / `let` names declared in `source`, deduplicated. */
function declaredIn(source: string): Completion[] {
  const seen = new Set<string>();
  const out: Completion[] = [];
  for (const line of source.split('\n')) {
    const m = DECLARATION.exec(line);
    if (!m) continue;
    const [, keyword, name] = m;
    if (seen.has(name)) continue;
    seen.add(name);
    const type =
      keyword === 'fn' ? KIND.fn : keyword === 'struct' ? KIND.struct : KIND.variable;
    // Boosted above the vocabulary: what you wrote in this file is what you mean more
    // often than a builtin you have not called yet.
    out.push({ label: name, type, detail: `${keyword} — declared in this file`, boost: 2 });
  }
  return out;
}

// ── Completion ────────────────────────────────────────────────────────────────

const digCompletion: CompletionSource = (ctx: CompletionContext): CompletionResult | null => {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const upToCaret = line.text.slice(0, ctx.pos - line.from);

  // An `import` line has nothing Bennu can honestly offer — see the module doc.
  if (/^[ \t]*(import|from)\b/.test(upToCaret)) return null;

  const dotted = DOTTED.exec(upToCaret);
  if (dotted) {
    const [, object, prefix] = dotted;
    const options = MEMBER_OPTIONS[object] ?? METHOD_OPTIONS;
    if (!options.length) return null;
    return { from: ctx.pos - prefix.length, options, validFor: VALID_FOR };
  }

  // A `.` the pattern above didn't claim — `pos()[0].`, `3.`, a chained `a.b.`. The
  // receiver isn't a name, so there is nothing to look up; offering the whole vocabulary
  // there would just be noise the editor typed at you. (The core fires completion on every
  // typed `.`, so this branch is reached often.)
  if (upToCaret.endsWith('.')) return null;

  // A bare word. `matchBefore` with an end-anchored `*` pattern always matches — an empty
  // one at the caret — so the gate is its *width*: no identifier under way means the popup
  // only opens on an explicit request (Ctrl+Space), never on typing `(` or a space.
  const word = ctx.matchBefore(WORD_BEFORE);
  const typing = !!word && word.from < word.to;
  if (!typing && !ctx.explicit) return null;
  return {
    from: word ? word.from : ctx.pos,
    options: [...declaredIn(ctx.state.doc.toString()), ...STATIC_OPTIONS],
    validFor: VALID_FOR,
  };
};

// ── Hover ─────────────────────────────────────────────────────────────────────

/**
 * The help for the word under the pointer.
 *
 * Qualified names are resolved **with** their namespace (`Speed.MAX_VALUE`, not
 * `MAX_VALUE`) because `MIN_VALUE` / `MAX_VALUE` exist in both `Tick` and `Speed`: a
 * bare lookup would always show the first one's text, which is the kind of wrong that
 * reads as right.
 *
 * Silent — `null` — over anything the language doesn't own. Explaining an unrelated
 * builtin over a name the user invented is worse than showing nothing.
 */
function digHover(view: EditorView, pos: number): Tooltip | null {
  const line = view.state.doc.lineAt(pos);
  const text = line.text;
  const rel = pos - line.from;

  let s = rel;
  let e = rel;
  while (s > 0 && WORD.test(text[s - 1])) s--;
  while (e < text.length && WORD.test(text[e])) e++;
  if (s === e) return null;
  const word = text.slice(s, e);

  // The qualifier, when the word is `<object>.<word>`.
  const before = text.slice(0, s);
  const qualifier = /([A-Za-z_][A-Za-z0-9_]*)\.$/.exec(before)?.[1];

  let signature: string;
  let doc: string;
  let container: string | undefined;
  let kind: string;

  if (qualifier && DIG_CATALOG.namespaces[qualifier]) {
    const entry = lookupMember(DIG_CATALOG, qualifier, word);
    if (!entry) return null;
    ({ signature, doc } = entry);
    container = qualifier;
    kind = 'value';
  } else if (qualifier) {
    // A method on something whose type we don't know: show every receiver it exists on.
    const matches = lookupMethod(DIG_CATALOG, word);
    if (!matches.length) return null;
    signature = matches.map((m) => m.entry.signature).join('   ·   ');
    doc = matches.map((m) => m.entry.doc).join('\n\n');
    container = matches.map((m) => m.kind).join(' / ');
    kind = 'method';
  } else {
    const entry = lookupWord(DIG_CATALOG, word);
    if (!entry) return null;
    ({ signature, doc } = entry);
    kind = DIG_CATALOG.keywords[word]
      ? 'keyword'
      : DIG_CATALOG.builtins[word]
        ? 'builtin'
        : 'namespace';
  }

  // The signature already opens the doc; the card would otherwise print it twice.
  const body = doc.slice(signature.length).trim();
  const from = line.from + s;
  return {
    pos: from,
    end: line.from + e,
    above: true,
    create() {
      return { dom: hoverCardDom({ signature, container, kind, doc: body }) };
    },
  };
}

/** The `.dig` intelligence: local completion + local hover. No backend, ever. */
export function createDigIntel(): CodeEditorIntel {
  return { completion: digCompletion, hover: digHover };
}
