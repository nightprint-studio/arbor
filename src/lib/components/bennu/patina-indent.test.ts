import { describe, expect, it } from 'vitest';
import { columnsOf, patinaIndentFor } from './patina-indent';

/** The indent of the line after the last `\n` of `text`, with `after` still to come on it. */
const next = (text: string, after = '') => patinaIndentFor(text, after, 4);

describe('Patina indentation', () => {
  it('goes one unit in after an opening brace and back out on a closer', () => {
    expect(next('fn f() {\n')).toBe(4);
    expect(next('fn f() {\n    if x {\n')).toBe(8);
    expect(next('fn f() {\n    if x {\n        a()\n', '}')).toBe(4);
    expect(next('fn f() {\n    a()\n', '}')).toBe(0);
  });

  it('indents from the line of the opener, not from its column', () => {
    expect(next('    foo(a,\n')).toBe(8);
    expect(next('let xs = [\n')).toBe(4);
    expect(next('    let xs = [\n        1,\n', ']')).toBe(4);
  });

  it('keeps the level inside a block, whatever the previous line holds', () => {
    expect(next('fn f() {\n    let a = 1\n')).toBe(4);
    expect(next('fn f() {\n    // a comment }\n    let a = "}"\n')).toBe(4);
  });

  it('puts an arm body one unit under the arrow, and the chain under its first line', () => {
    expect(next('match x {\n    A =>\n')).toBe(8);
    expect(next('match x {\n    A => {\n        a()\n    }\n')).toBe(4);
    expect(next('fn f() {\n    xs.iter()\n', '.map(f)')).toBe(8);
    expect(next('fn f() {\n    xs.iter()\n        .map(f)\n', '.count()')).toBe(8);
  });

  it('is 0 at the top level and ignores the whitespace already there', () => {
    expect(next('')).toBe(0);
    expect(next('use voriz::prelude::*\n')).toBe(0);
    expect(next('fn f() {}\n')).toBe(0);
  });

  it('reads a tab as the next tab stop', () => {
    expect(columnsOf('\t\tx', 4)).toBe(8);
    expect(columnsOf('  \tx', 4)).toBe(4);
  });
});
