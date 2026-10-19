import { describe, expect, it } from 'vitest';
import type { DiffHunk } from '$lib/ipc/bennu/history';
import { hunksTouching, revisionsWhereSelectionChanged, selectionSignature } from './selection-delta';

// Disk lines 10..13 were `a b c d`; this revision had `a X c d` — one line replaced at 11.
const REPLACED: DiffHunk = {
  old_start: 10,
  new_start: 10,
  lines: [
    { kind: 'context', old: 10, new: 10, text: 'a' },
    { kind: 'del', old: 11, text: 'X' },
    { kind: 'add', new: 11, text: 'b' },
    { kind: 'context', old: 12, new: 12, text: 'c' },
  ],
};

// A change far away, at line 40.
const FAR: DiffHunk = {
  old_start: 40,
  new_start: 40,
  lines: [
    { kind: 'del', old: 40, text: 'old' },
    { kind: 'add', new: 40, text: 'new' },
  ],
};

describe('selectionSignature', () => {
  it('is empty when nothing inside the range differs', () => {
    expect(selectionSignature([FAR], 10, 13)).toBe('');
    expect(selectionSignature([], 10, 13)).toBe('');
  });

  it('records the changes inside the range and ignores context', () => {
    const sig = selectionSignature([REPLACED, FAR], 10, 13);
    expect(sig).toContain('del11:X');
    expect(sig).toContain('add11:b');
    expect(sig).not.toContain('old');
  });

  it('places a deletion at the line that follows it', () => {
    const removed: DiffHunk = {
      old_start: 5,
      new_start: 5,
      lines: [{ kind: 'del', old: 5, text: 'gone' }, { kind: 'context', old: 6, new: 5, text: 'x' }],
    };
    expect(selectionSignature([removed], 5, 5)).toContain('del5:gone');
    expect(selectionSignature([removed], 6, 9)).toBe('');
  });
});

describe('hunksTouching', () => {
  it('keeps whole hunks that change something in range', () => {
    expect(hunksTouching([REPLACED, FAR], 11, 11)).toEqual([REPLACED]);
    expect(hunksTouching([REPLACED, FAR], 100, 120)).toEqual([]);
  });
});

describe('revisionsWhereSelectionChanged', () => {
  it('lists a state once per run of identical states', () => {
    const revs = ['r0', 'r1', 'r2', 'r3'];
    // r0 equals disk; r1 and r2 held the same older text; r3 older still.
    const sigs = ['', 'A', 'A', 'B'];
    expect(revisionsWhereSelectionChanged(revs, sigs)).toEqual(['r1', 'r3']);
  });

  it('lists nothing when the lines never changed', () => {
    expect(revisionsWhereSelectionChanged(['r0', 'r1'], ['', ''])).toEqual([]);
  });
});
