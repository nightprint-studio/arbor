/**
 * Where an Alt+Enter offer's selection lands once its edits are applied.
 *
 * An offer's `select` is expressed as bytes into the inserted text of one of its edits (see
 * `IntentionOffer.select`), because only that is unambiguous: every edit before the chosen one moves
 * where its text ends up. This resolves it to a byte range in the document **after** the edits, which
 * the editor's `selectByteRange` then maps to UTF-16 against the buffer it holds.
 *
 * Pure, so the arithmetic is tested without an editor.
 */

/** One byte-range replacement, offsets into the document as it was when the offer was asked. */
export interface OfferEdit {
  start: number;
  end: number;
  text: string;
}

/** Bytes `[start, end)` into the text of `edits[edit]`. */
export interface OfferSelect {
  edit: number;
  start: number;
  end: number;
}

const encoder = new TextEncoder();

function byteLength(text: string): number {
  return encoder.encode(text).length;
}

/** The offer's edits as one list: `edits` when it has several, else its single range as edit `0`. */
export function offerEdits(offer: {
  start: number;
  end: number;
  replacement: string;
  edits?: readonly OfferEdit[];
}): OfferEdit[] {
  return offer.edits?.length
    ? [...offer.edits]
    : [{ start: offer.start, end: offer.end, text: offer.replacement }];
}

/**
 * The selection as a byte range of the edited document, or `null` when it names no edit or reaches
 * past that edit's text — an offer the editor applies without selecting anything rather than one
 * that selects the wrong code.
 *
 * The edits are taken in the order the editor applies them in one transaction: by start, and in list
 * order among edits that start at the same byte (an import and an annotation both inserted at 0).
 */
export function selectionAfterEdits(
  edits: readonly OfferEdit[],
  select: OfferSelect,
): { start: number; end: number } | null {
  const chosen = edits[select.edit];
  if (!chosen) return null;
  if (select.start < 0 || select.start > select.end || select.end > byteLength(chosen.text)) {
    return null;
  }
  let shift = 0;
  edits.forEach((edit, index) => {
    const before = edit.start < chosen.start || (edit.start === chosen.start && index < select.edit);
    if (before) shift += byteLength(edit.text) - (edit.end - edit.start);
  });
  const landed = chosen.start + shift;
  return { start: landed + select.start, end: landed + select.end };
}
