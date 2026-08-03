/**
 * Bennu editor file-kind predicates — the single source of truth for "which editor
 * actions apply to this file".
 *
 * Used to gate the command palette, the editor context menu, and the keyboard
 * shortcuts so Bennu never *offers* (or fires) an action that's meaningless for the
 * open file: no "Generate…" / Java intentions on a `.jsp` or `.xml`, no go-to /
 * rename / find-usages on a plain `.txt` / `.md` / `.properties`. One place to
 * change if a new navigable file type is added.
 */

/** Lower-cased extension of `path` (without the dot), or '' when there is none. */
function ext(path: string | null | undefined): string {
  if (!path) return '';
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
}

/** A `.java` source — the only kind that supports Generate + the Java intentions. */
export function isJavaFile(path: string | null | undefined): boolean {
  return ext(path) === 'java';
}

/** A JSP-family file (page / fragment / tag file). */
export function isJspFile(path: string | null | undefined): boolean {
  return ['jsp', 'jspf', 'tag', 'tagx'].includes(ext(path));
}

/** An XML-family config file (struts / spring / tiles / mybatis / validation / pom / tld …). */
export function isXmlFile(path: string | null | undefined): boolean {
  return ['xml', 'xsd', 'wsdl', 'xsl', 'xslt', 'tld', 'pom', 'iml', 'fxml'].includes(ext(path));
}

/** A Rust source. Highlighted only — see {@link supportsCodeNav}. */
export function isRustFile(path: string | null | undefined): boolean {
  return ext(path) === 'rs';
}

/** A geode `.dig` mole script. Highlighted, folded, and completed from its (closed)
 *  vocabulary — but its navigation is a later story, so not in {@link supportsCodeNav}. */
export function isDigFile(path: string | null | undefined): boolean {
  return ext(path) === 'dig';
}

/**
 * True when Bennu's **Java analyzers** understand the file at all — Java (symbols), JSP
 * (page vars + action refs + includes), and config XML (`class="…"` / bean ids / mapper
 * statements). This is the one list; the two predicates below name the two things callers
 * actually want to know, so the set can grow in a single place.
 *
 * `.rs`, `.dig`, `.toml`, `.sql` and the plain text kinds are out: they are *edited* in
 * Bennu (highlighted, and `.dig` completed) but no analyzer in `bennu-be` can say anything
 * about them.
 */
function isAnalyzedFile(path: string | null | undefined): boolean {
  return isJavaFile(path) || isJspFile(path) || isXmlFile(path);
}

/**
 * True when the file supports semantic navigation — go-to declaration, find usages,
 * rename, file structure.
 *
 * `.rs` and `.dig` are deliberately out. Both are highlighted, and `.dig` even completes
 * — but resolving a declaration across files needs an index neither has (an LSP for Rust,
 * a cross-file symbol pass for `.dig`). A go-to that silently does nothing is worse than
 * an action that isn't offered.
 */
export function supportsCodeNav(path: string | null | undefined): boolean {
  return isAnalyzedFile(path);
}

/**
 * True when it is worth asking `bennu_diagnostics` about the file.
 *
 * Same set as {@link supportsCodeNav} today, and named separately because the *reason*
 * differs: navigation is hidden for lack of an index, whereas a validation request for a
 * `.rs` file would hand a Rust buffer to a Java validator — a round-trip per keystroke
 * whose best possible outcome is an empty list.
 */
export function supportsDiagnostics(path: string | null | undefined): boolean {
  return isAnalyzedFile(path);
}
