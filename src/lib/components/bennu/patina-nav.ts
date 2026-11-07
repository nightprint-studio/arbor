/**
 * What Patina (`.ptn`) navigation needs that is not the engine's: which buffers a go-to is asked of, and which targets are not the
 * project's to edit.
 *
 * Kept out of the editor component so it can be tested: the gesture used to skip `.ptn` silently (the check for "is there an engine
 * for this file" knew `.java` and the language-server files, nothing else), and a gesture that does nothing is not an error anybody sees.
 */

/** A `.ptn` path. The same test as `isPatinaFile` in `file-kind.ts`, which cannot be imported here: that module pulls in the language-server store, and this one is meant to load on its own. */
function isPtn(path: string | null | undefined): boolean {
  return !!path && /\.ptn$/i.test(path);
}

/** Whether `bennu_declaration` is the right question for this buffer: Java (the index), a language-server file, or a Patina script
 *  (the `language-intel` extension that serves it). Anything else would go to the Java resolver, which parses whatever it is given as Java. */
export function hasDeclarationEngine(path: string | null | undefined, isJava: boolean, isLsp: boolean): boolean {
  return isJava || isLsp || isPtn(path);
}

function canon(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/**
 * Whether a `.ptn` go-to target is somebody else's file to read and not the open project's to edit: a mod this one depends on that lives
 * beside it (outside the open folder), or a library view the backend wrote out under its data folder (`…/decompiled/patina/…`).
 *
 * Without a project root there is nothing to compare with, and the answer is `false` — an editable file is the old behaviour.
 */
export function opensReadOnly(target: string, projectRoot: string | null | undefined): boolean {
  const t = canon(target);
  if (t.includes('/decompiled/patina/')) return true;
  if (!projectRoot) return false;
  const r = canon(projectRoot);
  return !(t === r || t.startsWith(`${r}/`));
}
