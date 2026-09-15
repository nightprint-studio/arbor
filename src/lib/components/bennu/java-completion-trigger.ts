/**
 * When the Java completion source asks the backend, and what it may add to the answer on its own.
 *
 * Pure functions over the text of the line before the caret, so the decisions the popup makes on
 * every keystroke can be tested without an editor — which is how they went wrong unnoticed: the
 * "after a dot" test used to be `matchBefore(/\.$/)`, true only while NOTHING had been typed after
 * the dot. With `resolve_identity().ma|` it was false, so Java keywords and the buffer's own words
 * were appended to a member list where none of them can be written.
 */

/** Where the word being completed sits. */
export type CompletionSite =
  /** After `.` or `::` — only a member of what is on the left can be written. */
  | 'member'
  /** After `@` — an annotation type. */
  | 'annotation'
  /** Anywhere else. */
  | 'bare';

const WORD_AT_END = /[\w$]*$/;

/** The identifier being typed at the end of `textBefore` (possibly empty). */
export function wordBefore(textBefore: string): string {
  return WORD_AT_END.exec(textBefore)?.[0] ?? '';
}

/** What stands before the word being completed — whitespace between the two allowed, as Java allows
 *  it (`Type :: method`). */
export function completionSite(textBefore: string): CompletionSite {
  const head = textBefore
    .slice(0, textBefore.length - wordBefore(textBefore).length)
    .replace(/[ \t]+$/, '');
  if (head.endsWith('.') || head.endsWith('::')) return 'member';
  if (head.endsWith('@')) return 'annotation';
  return 'bare';
}

/**
 * Whether to ask for completions at all.
 *
 * Always on an explicit request (Ctrl+Space) and whenever a word is being typed. With nothing typed,
 * only right after one of the separators that narrow the answer on their own:
 *
 * - `.` and `::` — the members of what is on the left, a short list that is nearly always wanted;
 * - `@` — the only character in Java that cuts the legal names from every type on the classpath to
 *   the annotation types on it, so the popup at that instant is short and nearly always right.
 */
export function shouldAskForCompletion(textBefore: string, explicit: boolean): boolean {
  if (explicit || wordBefore(textBefore).length > 0) return true;
  return /(\.|::|@)$/.test(textBefore);
}

/**
 * Whether the editor may add its own guesses — Java keywords, words already in the buffer — to what
 * the backend answered. Never at a member position: after `.` or `::` a keyword does not compile,
 * and a word from elsewhere in the file is not a member of anything.
 */
export function offersFallbackWords(textBefore: string): boolean {
  return completionSite(textBefore) !== 'member';
}
