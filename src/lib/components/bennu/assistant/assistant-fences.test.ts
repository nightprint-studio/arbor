import { describe, expect, it } from 'vitest';
import { findFences, hasFence, inlineSpans, isInsideFence, removeFence, splitFences } from './assistant-fences';

const MSG = 'Why?\n\n```java\nint a = 1;\nint b = 2;\n```\n\nand then\n\n```\nplain\n```\n';

describe('findFences', () => {
  it('finds the blocks, their language and size', () => {
    const [a, b] = findFences(MSG);
    expect(a).toMatchObject({ language: 'java', code: 'int a = 1;\nint b = 2;', lines: 2, closed: true });
    expect(b).toMatchObject({ language: '', code: 'plain', lines: 1 });
  });
  it('treats an unclosed fence as running to the end', () => {
    const [a] = findFences('x\n```rust\nfn a() {}');
    expect(a).toMatchObject({ language: 'rust', code: 'fn a() {}', closed: false });
  });
  it('needs a closing fence at least as long', () => {
    const [a] = findFences('````md\n```\ninner\n```\n````');
    expect(a.code).toBe('```\ninner\n```');
  });
  it('is empty without a fence', () => {
    expect(findFences('no code here')).toEqual([]);
    expect(hasFence('a `b` c')).toBe(false);
  });
});

describe('splitFences', () => {
  it('alternates prose and code', () => {
    const s = splitFences(MSG);
    expect(s.map((x) => x.kind)).toEqual(['text', 'code', 'text', 'code']);
    expect(s[0]).toEqual({ kind: 'text', text: 'Why?' });
  });
});

describe('isInsideFence', () => {
  it('is true between the fences and false outside', () => {
    const text = 'a\n```js\nx\n```\nb';
    expect(isInsideFence(text, text.indexOf('x'))).toBe(true);
    expect(isInsideFence(text, 1)).toBe(false);
    expect(isInsideFence(text, text.length)).toBe(false);
  });
  it('is true at the end of an unclosed fence', () => {
    const text = 'a\n```js\nx';
    expect(isInsideFence(text, text.length)).toBe(true);
  });
});

describe('removeFence', () => {
  it('drops a block and closes the gap', () => {
    const [a] = findFences(MSG);
    expect(removeFence(MSG, a)).toBe('Why?\n\nand then\n\n```\nplain\n```\n');
  });
});

describe('inlineSpans', () => {
  it('splits code and bold', () => {
    expect(inlineSpans('a `b` **c**')).toEqual([
      { code: false, bold: false, text: 'a ' }, { code: true, bold: false, text: 'b' },
      { code: false, bold: false, text: ' ' }, { code: false, bold: true, text: 'c' },
    ]);
  });
});
