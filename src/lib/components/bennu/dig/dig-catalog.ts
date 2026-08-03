/**
 * The `.dig` vocabulary, as Bennu reads it — the shape of the generated
 * {@link import('./catalog').DIG_CATALOG} plus the lookups completion and hover need.
 *
 * `.dig` is the language of **geode** (`/games/geode`): a Python-shaped scripting
 * language for the mole, with a fixed set of host builtins (`seed`, `harvest`, `scan`,
 * …), dotted value namespaces (`Crystal.Amethyst`, `Tool.Pick`) and two collection
 * method sets. It has no LSP and doesn't need one for what Bennu offers: the vocabulary
 * is closed, so completion over it is a lookup, not an inference.
 *
 * ## The format is a contract
 *
 * Every entry's **first line is the signature** (`harvest()`, `move(direction)`,
 * `while <condizione>:`) and the rest is the explanation, with examples indented four
 * spaces. geode enforces that with a test because the two halves land in two different
 * places: the signature is the completion item's `detail` (one line is all a dropdown
 * row has), the whole thing is the hover. {@link splitEntry} is the one place that
 * split happens.
 *
 * ## What is deliberately absent
 *
 * No type inference, so a `.` after a local variable cannot know whether it holds a
 * list or a map. Rather than guess, Bennu offers **both** method sets and labels each
 * with its receiver (`lista.append` / `mappa.has`) — the reader picks, and nothing has
 * been asserted that could be wrong. Same reasoning as the rest of Arbor's completion:
 * absent beats plausibly wrong.
 */

/** One namespace of dotted values (`Tool`, `Crystal`, `Tick`, `Speed`, `Item`). */
export interface DigNamespace {
  /** The namespace's own description (geode's reserved `about` key). */
  about: string;
  /** Member name → its help text (`Pick` → `Tool.Pick\n…`). */
  members: Record<string, string>;
}

/** The generated catalog. Keys are names as written in a `.dig`; values are the full
 *  help text, first line = signature (see the module doc). */
export interface DigCatalog {
  /** Which geode locale the help text was generated from — `en`, matching the rest of
   *  Arbor. geode itself defaults to `it`; the generator's `--lang` is what bridges them. */
  language: string;
  /** Host builtins — the closed set a `.dig` may call. */
  builtins: Record<string, string>;
  /** Reserved words, including the three literals (`true` / `false` / `none`). */
  keywords: Record<string, string>;
  /** Dotted value namespaces, by namespace name. */
  namespaces: Record<string, DigNamespace>;
  /** Collection methods by receiver kind (`list` / `map`). */
  methods: Record<string, Record<string, string>>;
}

/** An entry split into the two halves its format promises. */
export interface DigEntry {
  /** The first line — the syntactic form. What a dropdown row shows. */
  signature: string;
  /** The whole text, signature included. What a hover shows. */
  doc: string;
}

/** Split an entry into signature + full doc. An entry that is a single line (a missing
 *  translation shows up as the bare i18n key) yields the same string for both, which
 *  degrades visibly instead of silently. */
export function splitEntry(doc: string): DigEntry {
  const signature = doc.split('\n', 1)[0] ?? doc;
  return { signature, doc };
}

/**
 * The help for a bare word — a builtin, else a keyword, else a namespace name.
 * `null` when the word isn't part of the language, which is the answer that matters:
 * hover must stay **silent** over a name the user invented rather than explain some
 * unrelated builtin.
 */
export function lookupWord(catalog: DigCatalog, word: string): DigEntry | null {
  const doc =
    catalog.builtins[word] ?? catalog.keywords[word] ?? catalog.namespaces[word]?.about;
  return doc ? splitEntry(doc) : null;
}

/**
 * The help for a **qualified** member (`Tool.Pick`, `Speed.MAX_VALUE`).
 *
 * Qualified and not bare on purpose: `MIN_VALUE` exists in both `Tick` and `Speed`, and
 * looking it up by bare name would always show the first one's doc. Carrying the
 * namespace makes that mistake impossible rather than unlikely.
 */
export function lookupMember(
  catalog: DigCatalog,
  namespace: string,
  member: string,
): DigEntry | null {
  const doc = catalog.namespaces[namespace]?.members[member];
  return doc ? splitEntry(doc) : null;
}

/**
 * The help for a collection method by bare name, across both receivers.
 *
 * Returns every match (`has` is both a list and a map method) so a caller can show them
 * together instead of picking one — without inference there is nothing to pick on.
 */
export function lookupMethod(
  catalog: DigCatalog,
  method: string,
): { kind: string; entry: DigEntry }[] {
  const out: { kind: string; entry: DigEntry }[] = [];
  for (const [kind, table] of Object.entries(catalog.methods)) {
    const doc = table[method];
    if (doc) out.push({ kind, entry: splitEntry(doc) });
  }
  return out;
}
