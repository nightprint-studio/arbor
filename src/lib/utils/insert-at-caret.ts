/**
 * Insert dictated text into a field's value at the caret (or over the selection), padding with a
 * space only where two words would otherwise run together. Pure, so the spacing rules are tested.
 */
export function insertAtCaret(value: string, start: number, end: number, text: string): { value: string; caret: number } {
  const s = Math.max(0, Math.min(start, value.length));
  const e = Math.max(s, Math.min(end, value.length));
  const piece = text.trim();
  if (!piece) return { value, caret: s };
  const before = value.slice(0, s);
  const after = value.slice(e);
  const lead = before && !/\s$/.test(before) ? ' ' : '';
  const trail = after && !/^[\s.,;:!?)]/.test(after) ? ' ' : '';
  const inserted = lead + piece + trail;
  return { value: before + inserted + after, caret: before.length + lead.length + piece.length + trail.length };
}
