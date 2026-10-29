import { describe, expect, it } from 'vitest';
import { insertAtCaret } from './insert-at-caret';

describe('insertAtCaret', () => {
  it('appends to an empty field without padding', () => {
    expect(insertAtCaret('', 0, 0, ' hello ')).toEqual({ value: 'hello', caret: 5 });
  });

  it('adds a space only between words', () => {
    expect(insertAtCaret('fix', 3, 3, 'the bug').value).toBe('fix the bug');
    expect(insertAtCaret('fix ', 4, 4, 'the bug').value).toBe('fix the bug');
    expect(insertAtCaret('a.', 1, 1, 'b').value).toBe('a b.');
  });

  it('replaces a selection and puts the caret after the text', () => {
    const r = insertAtCaret('rename foo now', 7, 10, 'bar');
    expect(r.value).toBe('rename bar now');
    expect(r.caret).toBe(10);
  });

  it('ignores empty dictation and out-of-range carets', () => {
    expect(insertAtCaret('abc', 1, 1, '   ')).toEqual({ value: 'abc', caret: 1 });
    expect(insertAtCaret('abc', 99, 120, 'x').value).toBe('abc x');
  });
});
