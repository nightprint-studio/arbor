import { describe, expect, it } from 'vitest';
import { minimalChange } from './minimal-change';

const apply = (s: string, c: { from: number; to: number; insert: string }) =>
  s.slice(0, c.from) + c.insert + s.slice(c.to);

describe('minimalChange', () => {
  it('replaces only the differing middle', () => {
    const c = minimalChange('class A {\n  int x;\n}\n', 'class A {\n  long x;\n}\n');
    expect(c).toEqual({ from: 12, to: 15, insert: 'long' });
  });

  it('is empty for equal strings', () => {
    const c = minimalChange('abc', 'abc');
    expect(c.from).toBe(c.to);
    expect(c.insert).toBe('');
  });

  it('handles insertion, deletion and a full rewrite', () => {
    for (const [a, b] of [['', 'abc'], ['abc', ''], ['abc', 'xyz'], ['aaa', 'aaaa'], ['aaaa', 'aaa']]) {
      expect(apply(a, minimalChange(a, b))).toBe(b);
    }
  });

  it('never splits a surrogate pair', () => {
    const a = 'x\u{1F600}y';
    const b = 'x\u{1F601}y';
    const c = minimalChange(a, b);
    expect(apply(a, c)).toBe(b);
    expect(c.from).toBe(1);
    expect(c.to).toBe(3);
  });
});
