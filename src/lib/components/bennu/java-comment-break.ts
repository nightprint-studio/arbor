/**
 * What Enter writes inside a Java comment — the ` * ` of a Javadoc, the closing ` *\/` of one just
 * opened, the `// ` of a line comment split in two.
 *
 * Pure functions over the caret's line, so the shapes can be tested without an editor; the editor
 * command in `java-newline.ts` supplies the lexical context and applies the answer.
 */

/** The caret, inside a comment. */
export interface CommentBreakInput {
  /** The caret's line up to the caret, trailing whitespace included. */
  lineBefore: string;
  /** The caret's line from the caret on. */
  lineAfter: string;
  /** Column in the caret's line where the comment opens, or -1 when it opened on an earlier line. */
  openerAt: number;
  /** `/* … *\/` rather than `// …`. */
  block: boolean;
  /** Whether the block comment is already closed somewhere after the caret. */
  closed: boolean;
}

/** The lines Enter produces. */
export interface CommentBreak {
  /** What the new caret line starts with, the caret right after it. */
  lead: string;
  /** A line to add below the caret's, closing the comment — or `null`. */
  close: string | null;
}

/** What Enter writes in a comment, or `null` when the break is an ordinary code break (after a line
 *  comment with nothing left to carry over). */
export function commentBreak(input: CommentBreakInput): CommentBreak | null {
  const { lineBefore, lineAfter, openerAt, block } = input;
  // Between the two characters of the opener is not inside the comment yet.
  if (openerAt >= 0 && lineBefore.length < openerAt + 2) return null;
  const leading = /^[ \t]*/.exec(lineBefore)![0];

  if (!block) {
    return lineAfter.trim() === '' ? null : { lead: `${leading}// `, close: null };
  }

  const head = lineAfter.trimStart();
  let margin: string;
  let starred: boolean;
  const starLine = /^([ \t]*)\*(?!\/)/.exec(lineBefore);
  if (openerAt >= 0) {
    // Under the `*` of `/*`, whatever stands before the opener on its line.
    margin = `${lineBefore.slice(0, openerAt).replace(/[^\t]/g, ' ')} `;
    starred = true;
  } else if (starLine) {
    margin = starLine[1];
    starred = true;
  } else {
    // A comment written without a star column keeps its own margin, and gets none added.
    margin = leading;
    starred = false;
  }

  const lead = margin + (starred && !head.startsWith('*/') ? '* ' : '');
  const justOpened = openerAt >= 0 && /^\/\*\*?$/.test(lineBefore.slice(openerAt).trimEnd());
  return { lead, close: justOpened && !input.closed ? `${margin}*/` : null };
}

/**
 * Whether the block comment the caret is in is closed further on, given the text after the caret.
 *
 * An unclosed `/*` swallows the rest of the file, so "closed" means a `*\/` arrives before any other
 * comment opens: the one after that belongs to the next comment, not to this one.
 */
export function isBlockCommentClosed(textAfter: string): boolean {
  const close = textAfter.indexOf('*/');
  if (close < 0) return false;
  const nextOpen = textAfter.indexOf('/*');
  return nextOpen < 0 || close < nextOpen;
}
