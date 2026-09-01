/**
 * The `.dig` vocabulary, as Bennu reads it — the shape of the generated
 * {@link import('./catalog').DIG_CATALOG} plus the lookups completion and hover need.
 *
 * `.dig` is the language of **geode** (`/games/geode`): a Python-shaped scripting
 * language for the mole, with a fixed set of host builtins (`seed`, `harvest`, `scan`,
 * …), dotted value namespaces (`Crystal.Amethyst`, `Tool.Pick`) and two collection
 * method sets.
 *
 * ## ⚠️ Only the NAMES are read now
 *
 * This described a catalog that answered completion and hover locally. It no longer
 * does: `.dig` is served by **nd-dig-lsp**, geode's own language server, and the help
 * text travels over the wire from the same `.toml` files this was generated from.
 *
 * What still reads the catalog is the **highlighter** ({@link
 * import('./dig-lang').digLanguage}), and it reads two key sets and nothing else: the
 * reserved words, and whether a bare identifier is a builtin. Both are name lookups.
 *
 * The help text in the generated file is therefore **dead weight** — harmless, but a
 * second copy of prose that now has an authoritative source. Slimming the generator to
 * emit names only is the obvious follow-up; it is left as one rather than done in
 * passing, because it means re-running `scripts/gen-dig-catalog.mjs`.
 *
 * ## The format is still a contract
 *
 * Every entry's **first line is the signature** (`harvest()`, `move(direction)`) and the
 * rest is the explanation, with examples indented four spaces. geode enforces that with
 * a test, and the server relies on it for exactly the same split this file used to do:
 * signature into the dropdown row, whole text into the hover.
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
