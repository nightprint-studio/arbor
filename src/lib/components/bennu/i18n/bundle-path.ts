/**
 * Is this path a fulcrum translation file — and of what.
 *
 * A frontend copy of `bennu_fulcrum_i18n::studio::bundle_of`, and the duplication is deliberate: the
 * editor has to decide *whether to ask the backend at all* before it can ask, and paying a round trip
 * on every `.toml` in the project to be told "no" would put the answer behind the question. The rule
 * is four lines of path arithmetic and it is the backend's business to be right about it — this side
 * only ever decides whether to ask, so the failure mode of a disagreement is one wasted call or one
 * missing colour, never a wrong offset.
 *
 * Keep the two in step. If the layout rule changes there, change it here.
 */

/** A translation file: which `i18n/` tree, which language, which category. */
export interface Bundle {
  /** The `i18n` directory, forward-slashed, no trailing slash. */
  root: string;
  /** The directory name — `it`, `en`. */
  lang: string;
  /** The file name without `.toml`, which is the label's category. */
  category: string;
}

/** The three files that *declare* what a translation may name; none of them is a translation. */
const DECLARING = ['languages.toml', 'styles.toml', 'glossary.toml'];

/**
 * `null` unless `path` is `…/i18n/<lang>/<category>.toml`.
 *
 * Nothing at the root of `i18n/` is a translation — the language *is* the directory, so a file
 * without one cannot be a translation of anything — and nothing deeper than one level is either,
 * because the engine does not load it.
 */
export function bundleOf(path: string): Bundle | null {
  const p = path.replace(/\\/g, '/');
  if (!p.endsWith('.toml')) return null;
  // The LAST `i18n/`, so a project that happens to live under a directory called `i18n` does not
  // make its whole tree one.
  const at = p.lastIndexOf('/i18n/');
  if (at < 0) return null;
  const root = p.slice(0, at + 5);
  const rest = p.slice(at + 6);
  const parts = rest.split('/');
  if (parts.length !== 2) return null;
  const [lang, name] = parts;
  if (!lang || DECLARING.includes(name)) return null;
  return { root, lang, category: name.slice(0, -'.toml'.length) };
}

/** Whether the editor should ask the backend about this file's markup. */
export function isI18nBundle(path: string | null | undefined): boolean {
  return !!path && bundleOf(path) !== null;
}
