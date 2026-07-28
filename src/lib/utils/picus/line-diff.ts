/**
 * The line diff the patch cards render.
 *
 * Deliberately **not** a general diff algorithm. `picus-rewrite` writes by byte
 * splicing over the original bytes — one contiguous region replaced, everything
 * else byte-identical — so the honest description of a Picus write is a common
 * prefix, a common suffix, and one changed region between them. Computing an LCS
 * to rediscover that would cost more and say the same thing, and it would also
 * invite the card to *look* like it understands rearrangements it never gets.
 *
 * A created file is the degenerate case: no prefix, no suffix, everything added.
 */

export interface LineDiff {
  /** 1-based line, in the BEFORE text, where the changed region starts. */
  startLine: number;
  /** Lines immediately before the change, for orientation. */
  contextBefore: string[];
  removed: string[];
  added: string[];
  /** Lines immediately after the change. */
  contextAfter: string[];
  /** True when the two texts are byte-identical — a target the write would not move. */
  unchanged: boolean;
}

/** Split keeping neither the trailing empty line nor the line-ending flavour. */
function lines(text: string): string[] {
  if (text === '') return [];
  const out = text.replace(/\r\n/g, '\n').split('\n');
  // A file ending in a newline yields a trailing '' that is not a line anyone wrote.
  if (out.length && out[out.length - 1] === '') out.pop();
  return out;
}

/**
 * Describe `before` → `after` as one replaced region with surrounding context.
 *
 * `context` is how many unchanged lines to keep on each side; three is enough to
 * recognise the place without turning the card into the file.
 */
export function spliceDiff(before: string, after: string, context = 3): LineDiff {
  const b = lines(before);
  const a = lines(after);

  let prefix = 0;
  while (prefix < b.length && prefix < a.length && b[prefix] === a[prefix]) prefix++;

  let suffix = 0;
  const maxSuffix = Math.min(b.length, a.length) - prefix;
  while (
    suffix < maxSuffix &&
    b[b.length - 1 - suffix] === a[a.length - 1 - suffix]
  ) {
    suffix++;
  }

  const removed = b.slice(prefix, b.length - suffix);
  const added = a.slice(prefix, a.length - suffix);

  return {
    startLine: prefix + 1,
    contextBefore: b.slice(Math.max(0, prefix - context), prefix),
    removed,
    added,
    contextAfter: b.slice(b.length - suffix, b.length - suffix + context),
    unchanged: removed.length === 0 && added.length === 0,
  };
}
