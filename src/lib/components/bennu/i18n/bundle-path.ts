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
  /** The file name without `.toml` — or, for a document, the folder under the language. */
  category: string;
  /** A `.md` document (`i18n/<lang>/<category>/…/<key>.md`): one literal value, no markup. */
  document: boolean;
}

/** The three files that *declare* what a translation may name; none of them is a translation. */
const DECLARING = ['languages.toml', 'styles.toml', 'glossary.toml'];

/**
 * `null` unless `path` is `…/i18n/<lang>/<category>.toml` or a `.md` document
 * `…/i18n/<lang>/<folder>/…/<name>.md`.
 *
 * Nothing at the root of `i18n/` is a translation — the language *is* the directory, so a file
 * without one cannot be a translation of anything — and a `.toml` deeper than one level is not
 * either, because the engine does not load it. A `.md` is loaded only inside a folder of a
 * language, and a name (file or folder) starting with a dot is skipped.
 */
export function bundleOf(path: string): Bundle | null {
  const p = path.replace(/\\/g, '/');
  const isToml = p.endsWith('.toml');
  if (!isToml && !p.endsWith('.md')) return null;
  // The LAST `i18n/`, so a project that happens to live under a directory called `i18n` does not
  // make its whole tree one.
  const at = p.lastIndexOf('/i18n/');
  if (at < 0) return null;
  const root = p.slice(0, at + 5);
  const parts = p.slice(at + 6).split('/');
  if (parts.some((part) => part.startsWith('.'))) return null;
  const name = parts[parts.length - 1];
  if (isToml) {
    if (parts.length !== 2) return null;
    const [lang] = parts;
    if (!lang || DECLARING.includes(name)) return null;
    return { root, lang, category: name.slice(0, -'.toml'.length), document: false };
  }
  if (parts.length < 3) return null;
  return { root, lang: parts[0], category: parts[1], document: true };
}

/** Whether the editor should ask the backend about this file's markup. */
export function isI18nBundle(path: string | null | undefined): boolean {
  return !!path && bundleOf(path) !== null;
}
