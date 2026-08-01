/**
 * Reading a note's name and folder out of the string that identifies it.
 *
 * **A note id is usually a path and is not always one.** `garrulus-vault` gives a
 * note the `uid` from its frontmatter when it has one and its vault-relative path
 * otherwise, and everything the index answers with — `Hit.id`, `VaultProblems.orphans`,
 * a `Backlink.from` — carries that id rather than a path. Four surfaces need to
 * turn it into something a human reads, so the rule lives here once: split it like
 * a path when it looks like one, and show it as it is when it does not.
 *
 * Shared by the dock panels and the search view. It sits under `panels/` because
 * that is where it was first needed; it belongs beside the note domain the day one
 * exists.
 */

/** Does this id read as a vault-relative path? A `uid` is an opaque token and
 *  splitting it on a slash it does not have would only ever produce itself. */
export function looksLikePath(id: string): boolean {
  return /[/\\]/.test(id) || /\.[A-Za-z0-9]+$/.test(id);
}

/** The note's name: the last path segment without its extension, or the id
 *  itself when the id is not a path. */
export function noteName(id: string): string {
  if (!looksLikePath(id)) return id;
  const last = id.split(/[/\\]/).pop() ?? id;
  return last.replace(/\.[^.]+$/, '');
}

/** The folder the note sits in, with a trailing slash so it reads as a location
 *  rather than as a second name. `''` for a note at the vault root, and for an id
 *  that is not a path. */
export function noteFolder(id: string): string {
  if (!looksLikePath(id)) return '';
  const parts = id.split(/[/\\]/);
  parts.pop();
  return parts.length > 0 ? `${parts.join('/')}/` : '';
}
