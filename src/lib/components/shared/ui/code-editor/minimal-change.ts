/**
 * The smallest single change that turns `before` into `after`: the common head and tail are kept,
 * only the middle is replaced.
 *
 * Why this exists: dispatching `{ from: 0, to: before.length, insert: after }` tells CodeMirror the
 * whole document was rewritten, so every position-bound thing (decorations, folds, the selection,
 * the scroll anchor) is mapped onto nothing. A reload that changed one line then redraws every
 * lens, hint and squiggle from scratch. With the head and tail untouched they ride along instead.
 */
export interface MinimalChange {
  from: number;
  to: number;
  insert: string;
}

const isHighSurrogate = (code: number) => code >= 0xd800 && code <= 0xdbff;
const isLowSurrogate = (code: number) => code >= 0xdc00 && code <= 0xdfff;

export function minimalChange(before: string, after: string): MinimalChange {
  const max = Math.min(before.length, after.length);
  let head = 0;
  while (head < max && before.charCodeAt(head) === after.charCodeAt(head)) head++;
  // Never split a surrogate pair: a change boundary between the halves would leave a lone surrogate
  // in the document for a moment.
  if (head > 0 && head < max + 1 && isHighSurrogate(before.charCodeAt(head - 1))) head--;

  let tail = 0;
  const room = max - head;
  while (
    tail < room &&
    before.charCodeAt(before.length - 1 - tail) === after.charCodeAt(after.length - 1 - tail)
  ) tail++;
  if (tail > 0 && isLowSurrogate(before.charCodeAt(before.length - tail))) tail--;

  return { from: head, to: before.length - tail, insert: after.slice(head, after.length - tail) };
}
