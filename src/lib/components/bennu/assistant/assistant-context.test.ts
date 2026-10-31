import { describe, expect, it } from 'vitest';
import { parseContext, serializeContext } from './assistant-context';

describe('assistant context list', () => {
  it('round-trips items, multi-line ones included', () => {
    const items = ['Answer in Italian', 'Keep code and comments in English,\nunless asked', 'Be terse'];
    expect(parseContext(serializeContext(items))).toEqual(items);
  });

  it('writes a bullet list and drops empty items', () => {
    expect(serializeContext(['One', '  ', '', 'Two'])).toBe('- One\n- Two');
    expect(serializeContext([])).toBe('');
  });

  it('reads free text written before the list: one item per paragraph', () => {
    expect(parseContext('Senior Java developer.\nStruts 1.\n\nNever touch /vendor.')).toEqual([
      'Senior Java developer.\nStruts 1.',
      'Never touch /vendor.',
    ]);
  });

  it('keeps a stray line before the first bullet as an item of its own', () => {
    expect(parseContext('Context:\n- one\n- two')).toEqual(['Context:', 'one', 'two']);
  });

  it('is empty for empty text', () => {
    expect(parseContext('')).toEqual([]);
    expect(parseContext('  \n\n')).toEqual([]);
  });
});
