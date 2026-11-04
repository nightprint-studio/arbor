/**
 * A pattern cut into runs that share one look, for the highlighted mirror behind the pattern field.
 *
 * Spans may nest (a group's parentheses inside the group's own colour); where they overlap the one
 * that starts later — the inner one — wins, which is the order {@link regexSpans} hands them over in.
 */

import type { RegexSpan } from './tokens';

export interface PaintSegment {
  text: string;
  /** Token class (`keyword`, `type`, …) or '' for plain text. */
  cls: string;
  /** The message of an `invalid` run. */
  message?: string;
  /** The run lies inside the range the pointer or caret is on. */
  active: boolean;
}

export function paintSegments(
  text: string, spans: RegexSpan[], active: { from: number; to: number } | null,
): PaintSegment[] {
  if (text === '') return [];
  const cls = new Array<string>(text.length).fill('');
  const bad = new Array<string | undefined>(text.length).fill(undefined);
  for (const s of spans) {
    for (let i = Math.max(0, s.from); i < Math.min(text.length, s.to); i++) {
      if (s.cls === 'invalid') bad[i] = s.message ?? 'Invalid';
      else cls[i] = s.cls;
    }
  }
  const out: PaintSegment[] = [];
  for (let i = 0; i < text.length; i++) {
    const on = !!active && i >= active.from && i < active.to;
    const last = out[out.length - 1];
    if (last && last.cls === cls[i] && last.message === bad[i] && last.active === on) last.text += text[i];
    else out.push({ text: text[i], cls: cls[i], message: bad[i], active: on });
  }
  return out;
}
