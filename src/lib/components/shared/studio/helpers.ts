/**
 * Shared helpers for the per-format StudioModal wrappers.
 *
 * Each wrapper used to keep its own copy of these (indentLabel, basename,
 * fmtBytes). They are trivial but appeared verbatim in five files — moved
 * here so the wrappers can shrink without losing the behaviour.
 */

/** Tab → "Tab", N-space string → "N sp". Used by the indent dropdown. */
export function indentLabel(unit: string): string {
  if (unit === '\t') return 'Tab';
  return `${unit.length} sp`;
}

/** Filesystem basename that works for both POSIX and Windows separators. */
export function basename(p: string | null | undefined): string {
  if (!p) return '';
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

/** Human-readable byte size — used by the modal header `size` chip. */
export function fmtBytes(n: number): string {
  if (n < 1024)              return `${n} B`;
  if (n < 1024 * 1024)       return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
