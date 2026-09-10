import { describe, it, expect } from 'vitest';

import { indentBody } from './snippet-stops';

/**
 * The half of a snippet insertion that is pure, and the half that is easy to get subtly wrong.
 *
 * A snippet body is written flush-left because it does not know where it will land. Every line
 * after the first therefore arrives at column 0 — a `psvm` three levels into a class reads as a
 * body that fell out of the code — and fixing that moves every tab stop after every newline, which
 * is what `shift` is for.
 */
describe('indentBody', () => {
  it('leaves a single-line body exactly as it is', () => {
    const body = indentBody('System.out.println();', '    ');
    expect(body.text).toBe('System.out.println();');
    expect(body.shift(7)).toBe(7);
  });

  it('leaves any body alone at the margin', () => {
    const body = indentBody('a\nb', '');
    expect(body.text).toBe('a\nb');
    expect(body.shift(2)).toBe(2);
  });

  it('indents every line after the first, and only those', () => {
    const body = indentBody('public static void main(String[] args) {\n    x\n}', '  ');
    expect(body.text).toBe('public static void main(String[] args) {\n      x\n  }');
  });

  it('moves a stop by the indentation added before it', () => {
    const body = indentBody('a\nb\nc', '\t');
    // Nothing added before the first line.
    expect(body.shift(0)).toBe(0);
    expect(body.shift(1)).toBe(1);
    // One indent before an offset on the second line, two before one on the third.
    expect(body.shift(2)).toBe(3);
    expect(body.shift(4)).toBe(6);
  });

  it('keeps a stop pointing at the same characters', () => {
    const text = 'for (int i = 0; i < n; i++) {\n    \n}';
    const stop = text.indexOf('n;');
    const body = indentBody(text, '        ');
    // The offset is on the FIRST line, so it does not move — and still names the same character.
    expect(body.text[body.shift(stop)]).toBe('n');
    // One on a later line moves, and still names the same character.
    const brace = text.lastIndexOf('}');
    expect(body.text[body.shift(brace)]).toBe('}');
  });

  it('indents a trailing newline too, which is where the caret would otherwise land', () => {
    const body = indentBody('try {\n}\n', '  ');
    expect(body.text).toBe('try {\n  }\n  ');
  });
});
