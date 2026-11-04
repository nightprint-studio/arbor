/**
 * The characters that differ between a name and its renamed form, for highlighting.
 *
 * `get_user_name` → `getUserName` changes the middle, not the ends, and showing the whole of both
 * names in the same colour makes the eye compare them letter by letter. A common prefix and suffix
 * are kept plain and only what is between them is marked — in both names, so the removed text and
 * the added text can each be coloured.
 *
 * Deliberately not a real diff: a rename is one identifier into another, and the middle is a better
 * answer to "what changed" than a minimal edit script that scatters marks through both names.
 */
export interface NameDiff {
  /** Unchanged text at the start of both names. */
  prefix: string;
  /** What the old name has between prefix and suffix. */
  removed: string;
  /** What the new name has there instead. */
  added: string;
  /** Unchanged text at the end of both names. */
  suffix: string;
}

/** The common prefix/suffix split of `from` and `to`. Surrogate pairs are never cut in half. */
export function diffNames(from: string, to: string): NameDiff {
  const a = [...from];
  const b = [...to];
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start += 1;
  let end = 0;
  while (
    end < a.length - start &&
    end < b.length - start &&
    a[a.length - 1 - end] === b[b.length - 1 - end]
  ) {
    end += 1;
  }
  return {
    prefix: a.slice(0, start).join(''),
    removed: a.slice(start, a.length - end).join(''),
    added: b.slice(start, b.length - end).join(''),
    suffix: a.slice(a.length - end).join(''),
  };
}
