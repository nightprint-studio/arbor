/**
 * Resolving an Alt+Enter offer's selection: the placeholder a fix wrote has to be the text selected,
 * wherever the offer's other edits pushed it.
 */

import { describe, expect, it } from 'vitest';
import { offerEdits, selectionAfterEdits, type OfferEdit } from './offer-selection';

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Apply byte edits the way one transaction does: all against the original, back to front. */
function apply(source: string, edits: readonly OfferEdit[]): string {
  let bytes = encoder.encode(source);
  const ordered = edits.map((e, i) => ({ e, i })).sort((a, b) => b.e.start - a.e.start || b.i - a.i);
  for (const { e } of ordered) {
    const text = encoder.encode(e.text);
    const next = new Uint8Array(bytes.length - (e.end - e.start) + text.length);
    next.set(bytes.subarray(0, e.start), 0);
    next.set(text, e.start);
    next.set(bytes.subarray(e.end), e.start + text.length);
    bytes = next;
  }
  return decoder.decode(bytes);
}

/** The text the selection covers in the edited source. */
function selectedText(source: string, edits: readonly OfferEdit[], select: { edit: number; start: number; end: number }) {
  const range = selectionAfterEdits(edits, select);
  if (!range) return null;
  return decoder.decode(encoder.encode(apply(source, edits)).subarray(range.start, range.end));
}

describe('offerEdits', () => {
  it('reads a single-range offer as its only edit', () => {
    expect(offerEdits({ start: 4, end: 4, replacement: ' = null' })).toEqual([{ start: 4, end: 4, text: ' = null' }]);
  });

  it('takes the edit list when the offer has one', () => {
    const edits = [{ start: 9, end: 9, text: 'b' }, { start: 1, end: 1, text: 'a' }];
    expect(offerEdits({ start: 9, end: 9, replacement: 'b', edits })).toEqual(edits);
  });
});

describe('selectionAfterEdits', () => {
  it('selects the placeholder an initializer wrote', () => {
    const source = 'final Api client;';
    const edits = [{ start: 16, end: 16, text: ' = null' }];
    expect(selectedText(source, edits, { edit: 0, start: 3, end: 7 })).toBe('null');
  });

  it('shifts past the edits that land before the chosen one, whatever order they are listed in', () => {
    const source = 'class A {\n  A() {\n  }\n  A(int x) {\n  }\n}\n';
    const first = source.indexOf('  }');
    const second = source.lastIndexOf('  }');
    // Descending, the way the backend sends them; the selection names the FIRST constructor's edit.
    const edits = [
      { start: second, end: second, text: '    this.s = null;\n' },
      { start: first, end: first, text: '    this.s = null;\n' },
    ];
    const range = selectionAfterEdits(edits, { edit: 1, start: 13, end: 17 })!;
    const out = apply(source, edits);
    expect(out.slice(range.start, range.end)).toBe('null');
    expect(range.start).toBe(out.indexOf('null'));
  });

  it('counts replaced text as well as inserted text', () => {
    // `int` → `long` grows the text before the chosen edit by one byte.
    const source = 'int x = 1; int y;';
    const edits = [
      { start: 0, end: 3, text: 'long' },
      { start: 16, end: 16, text: ' = 0L' },
    ];
    expect(selectedText(source, edits, { edit: 1, start: 3, end: 5 })).toBe('0L');
  });

  it('keeps list order among edits that start at the same byte', () => {
    const source = 'class A {}';
    const edits = [
      { start: 0, end: 0, text: 'import x.Y;\n' },
      { start: 0, end: 0, text: '@Y\n' },
    ];
    expect(selectedText(source, edits, { edit: 1, start: 0, end: 2 })).toBe('@Y');
  });

  it('measures in UTF-8 bytes, so text with accents before the edit does not skew it', () => {
    const source = '// perché\nfinal Api è;';
    const at = encoder.encode(source).length - 1;
    const edits = [
      { start: 0, end: 0, text: '// ü\n' },
      { start: at, end: at, text: ' = null' },
    ];
    expect(selectedText(source, edits, { edit: 1, start: 3, end: 7 })).toBe('null');
  });

  it('refuses a selection that names no edit or reaches past its text', () => {
    const edits = [{ start: 0, end: 0, text: ' = 0' }];
    expect(selectionAfterEdits(edits, { edit: 1, start: 0, end: 1 })).toBeNull();
    expect(selectionAfterEdits(edits, { edit: 0, start: 3, end: 9 })).toBeNull();
    expect(selectionAfterEdits(edits, { edit: 0, start: 3, end: 2 })).toBeNull();
  });
});
