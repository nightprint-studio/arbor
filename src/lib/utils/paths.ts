/**
 * Comparing file paths that came from two different places.
 *
 * The backend returns forward-slashed paths; the frontend's own state carries whatever the OS
 * handed it. On Windows those differ in separator *and* often in the drive letter's case, so
 * `a === b` is quietly false for the same file — and the bug it causes is never "paths look
 * wrong", it is a feature silently falling back to its default. A dialog opening on the wrong
 * entity, a tab failing to match itself, a target that reopens instead of scrolling.
 *
 * One function, because that class of bug is invisible until someone notices the default.
 */

/** Forward slashes, case-folded, no trailing separator. */
export function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/** Whether two paths name the same file. */
export function isSamePath(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  return normalizePath(a) === normalizePath(b);
}

/** The last segment of a path, for either separator. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
