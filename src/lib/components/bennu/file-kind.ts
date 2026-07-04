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

/**
 * True when the file supports semantic navigation — go-to declaration, find usages,
 * rename, file structure. Java (symbols), JSP (page vars + action refs + includes),
 * and config XML (`class="…"` / bean ids / mapper statements) all resolve; a plain
 * text / markdown / properties / yaml / json file does not, so those actions are
 * hidden there.
 */
export function supportsCodeNav(path: string | null | undefined): boolean {
  return isJavaFile(path) || isJspFile(path) || isXmlFile(path);
}
