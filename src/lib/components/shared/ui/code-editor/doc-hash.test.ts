import { describe, expect, it } from 'vitest';
import { docHash } from './doc-hash';

describe('docHash', () => {
  it('is the same for the same text and differs when one character does', () => {
    expect(docHash('class A {}')).toBe(docHash('class A {}'));
    expect(docHash('class A {}')).not.toBe(docHash('class B {}'));
  });

  it('tells a text from the same text with a line ending changed', () => {
    expect(docHash('a\nb')).not.toBe(docHash('a\r\nb'));
  });

  it('is an unsigned 32-bit number, so it crosses the wire as itself', () => {
    for (const text of ['', 'x', 'é'.repeat(1000)]) {
      const h = docHash(text);
      expect(Number.isInteger(h) && h >= 0 && h <= 0xffffffff).toBe(true);
    }
  });
});
