import { describe, expect, it } from 'vitest';
import { paintSegments } from './pattern-paint';
import { insertion } from './reference';
import { highlightRegex } from './tokens';

describe('pattern paint', () => {
  it('merges neighbours of one class and marks the emphasised range', () => {
    const text = '(a)+';
    const segs = paintSegments(text, highlightRegex(text, 'java'), { from: 3, to: 4 });
    expect(segs.map((s) => [s.text, s.cls, s.active])).toEqual([
      ['(', 'keyword', false], ['a', '', false], [')', 'keyword', false], ['+', 'operator', true],
    ]);
  });

  it('carries the parser message on an invalid run', () => {
    const segs = paintSegments('a(', highlightRegex('a(', 'java'), null);
    expect(segs.some((s) => s.message === 'Unclosed group')).toBe(true);
  });
});

describe('quick insert', () => {
  it('leaves the caret in the gap of a template token', () => {
    expect(insertion('(?:…)')).toEqual({ text: '(?:)', back: 1 });
    expect(insertion('\d  \D')).toEqual({ text: '\d', back: 0 });
  });
});
