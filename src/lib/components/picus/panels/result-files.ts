/**
 * Reading a file that is on its way into a cell.
 *
 * Pure, and separate from the flow that uses it, because these two decisions are
 * the ones worth being able to reason about on their own: which encoding a file
 * turned out to be, and whether its text fits the column it is headed for.
 */

/** What a file's bytes turned out to say, and how they were read. */
export interface DecodedFile {
  text: string;
  /** Named, never assumed — see below. */
  encoding: string;
}

/**
 * A file's bytes as text, and the encoding that produced it.
 *
 * UTF-8 first and **strictly**, so an invalid sequence is a failure rather than a
 * string of replacement characters; then windows-1252, which is not so much a guess
 * as the other thing these repositories are full of.
 *
 * Whichever it was is returned so the dialog can **name it**. This is the product
 * that exists because an encoding changed without anyone being told, and it is not
 * going to be the thing that does it.
 */
export function decode(bytes: Uint8Array): DecodedFile {
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    // A byte-order mark is a declaration about the file, not content — it must not
    // become the first character of the value.
    return { text: text.replace(/^﻿/, ''), encoding: 'UTF-8' };
  } catch {
    return { text: new TextDecoder('windows-1252').decode(bytes), encoding: 'windows-1252' };
  }
}

/**
 * The declared length in `varchar(255)`, when the type states one.
 *
 * `null` for a type with no length — `text`, `clob`, a bare `varchar` — which is a
 * different answer from zero and must not be shown as a limit of none.
 */
export function declaredLength(type: string): number | null {
  const found = /\(\s*(\d+)/.exec(type);
  const n = found ? Number(found[1]) : NaN;
  return Number.isFinite(n) ? n : null;
}
