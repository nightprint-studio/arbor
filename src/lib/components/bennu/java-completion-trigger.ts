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
export function shouldAskForCompletion(
  textBefore: string,
  explicit: boolean,
  /** Lazily: whether the caret is inside a string literal. Only asked when it can change the answer. */
  inString: () => boolean = () => false,
): boolean {
  if (explicit || wordBefore(textBefore).length > 0) return true;
  if (/(\.|::|@)$/.test(textBefore) || ANNOTATION_STRING_START.test(textBefore) || /\$\{$/.test(textBefore)) return true;
  // `:` opens a bind parameter in a query string — `where o.total > :|` — and means nothing elsewhere
  // in Java (a ternary, a label, an enhanced for), so it asks only inside a string.
  return textBefore.endsWith(':') && inString();
}

/** The opening quote of a string argument of an annotation — `@GetMapping("`, `@PostMapping(produces = "` — where the
 *  project's own values for that element are worth offering before a letter is typed. */
const ANNOTATION_STRING_START = /@[\w.]+\(\s*(?:\w+\s*=\s*)?\{?\s*"$/;

/** Whether the caret is inside an unfinished string in an annotation's parentheses — where the project's values and
 *  configuration keys are the answer, and words from elsewhere in the file are not. */
export function insideAnnotationString(textBefore: string): boolean {
  const open = textBefore.lastIndexOf('@');
  if (open < 0) return false;
  const tail = textBefore.slice(open);
  if (!/^@[\w.]+\s*\(/.test(tail) || tail.includes(')\n')) return false;
  return (tail.match(/(?<!\\)"/g)?.length ?? 0) % 2 === 1;
}

/**
 * Whether the editor may add its own guesses — Java keywords, words already in the buffer — to what
 * the backend answered. Never at a member position: after `.` or `::` a keyword does not compile,
 * and a word from elsewhere in the file is not a member of anything.
 */
export function offersFallbackWords(textBefore: string): boolean {
  return completionSite(textBefore) !== 'member' && !insideAnnotationString(textBefore);
}

/**
 * Whether the end of `before` (the document up to the caret) is inside a string literal — a plain
 * `"…"` or a text block `"""…"""`, across lines.
 *
 * The line-local quote count this replaces read every line of a text block as "outside", so a JPQL or
 * SQL query written the way modern Java writes them never reached the framework extension that knows
 * its entities and tables. Comments and character literals are skipped, so a quote inside them opens
 * nothing; a plain literal that is not closed on its line ends there, as the compiler would say.
 */
export function insideJavaString(before: string): boolean {
  const n = before.length;
  let i = 0;
  while (i < n) {
    const c = before[i];
    if (c === '/' && before[i + 1] === '/') {
      const end = before.indexOf('\n', i);
      if (end < 0) return false;
      i = end + 1;
    } else if (c === '/' && before[i + 1] === '*') {
      const end = before.indexOf('*/', i + 2);
      if (end < 0) return false;
      i = end + 2;
    } else if (c === '"' && before.startsWith('"""', i)) {
      let j = i + 3;
      for (;;) {
        if (j >= n) return true;
        if (before[j] === '\\') { j += 2; continue; }
        if (before.startsWith('"""', j)) { j += 3; break; }
        j++;
      }
      i = j;
    } else if (c === '"') {
      let j = i + 1;
      for (;;) {
        if (j >= n) return true;
        const d = before[j];
        if (d === '\\') { j += 2; continue; }
        if (d === '"') { j++; break; }
        if (d === '\n') break; // unterminated: the string ended with its line
        j++;
      }
      i = j;
    } else if (c === "'") {
      // A character literal (`'"'`, `'\''`): skipped whole so its quote opens nothing.
      const close = before[i + 1] === '\\' ? i + 3 : i + 2;
      i = before[close] === "'" ? close + 1 : i + 1;
    } else {
      i++;
    }
  }
  return false;
}
