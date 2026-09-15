import { describe, expect, it } from 'vitest';
import { MAX_PLACES, NavHistory, lineEditsOf, mapLine, type NavPlace } from './nav-ring';

const at = (file: string, line: number, col = 1): NavPlace => ({ file, line, col });

describe('recording', () => {
  it('Back returns to where a navigation started, Forward to where Back left', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 10));
    // The caret is now at the destination, B:200.
    expect(h.back(at('B.java', 200))).toEqual(at('A.java', 10));
    expect(h.forward(at('A.java', 10))).toEqual(at('B.java', 200));
    expect(h.canForward).toBe(false);
    expect(h.back(at('B.java', 200))).toEqual(at('A.java', 10));
  });

  it('merges consecutive origins on the same line of the same file', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 10, 3));
    h.leave(at('A.java', 10, 17));
    h.leave(at('A.java', 11));
    expect(h.snapshot().back).toEqual([at('A.java', 10, 17), at('A.java', 11)]);
  });

  it('does not merge nearby lines — a jump three lines away is still a jump', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 10));
    h.leave(at('A.java', 13));
    expect(h.snapshot().back).toHaveLength(2);
  });

  it('treats separators as the same file', () => {
    const h = new NavHistory();
    h.leave(at('src\\A.java', 10));
    h.leave(at('src/A.java', 10));
    expect(h.snapshot().back).toHaveLength(1);
  });

  it('a new navigation after a Back truncates Forward', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 1));
    h.leave(at('B.java', 1));
    h.back(at('C.java', 1)); // now at B, Forward = [C]
    expect(h.canForward).toBe(true);
    h.leave(at('B.java', 40)); // jump somewhere else from B
    expect(h.canForward).toBe(false);
  });

  it('caps the stack', () => {
    const h = new NavHistory();
    for (let i = 1; i <= MAX_PLACES + 20; i++) h.leave(at('A.java', i));
    expect(h.snapshot().back).toHaveLength(MAX_PLACES);
    expect(h.snapshot().back[0]).toEqual(at('A.java', 21));
  });
});

describe('Back and Forward never record', () => {
  it('walking back and forth leaves the stacks the same size', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 1));
    h.leave(at('B.java', 1));
    h.leave(at('C.java', 1));
    let here: NavPlace = at('D.java', 1);
    for (let i = 0; i < 3; i++) here = h.back(here)!;
    expect(here).toEqual(at('A.java', 1));
    for (let i = 0; i < 3; i++) here = h.forward(here)!;
    expect(here).toEqual(at('D.java', 1));
    expect(h.snapshot()).toEqual({
      back: [at('A.java', 1), at('B.java', 1), at('C.java', 1)],
      forward: [],
    });
  });

  it('Forward returns to where you moved to after arriving, not only where you landed', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 10));
    // Landed at B:200, then read down to B:350.
    expect(h.back(at('B.java', 350))).toEqual(at('A.java', 10));
    expect(h.forward(at('A.java', 10))).toEqual(at('B.java', 350));
  });

  it('skips a stop that is where you already are', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 5));
    h.leave(at('B.java', 9));
    // Ctrl+B onto a declaration on the same line: the top stop is the present.
    expect(h.back(at('B.java', 9, 30))).toEqual(at('A.java', 5));
  });

  it('returns null at either end and changes nothing', () => {
    const h = new NavHistory();
    expect(h.back(at('A.java', 1))).toBeNull();
    expect(h.forward(at('A.java', 1))).toBeNull();
    expect(h.snapshot()).toEqual({ back: [], forward: [] });
  });
});

describe('mapping through edits', () => {
  it('shifts lines below an insertion, keeps lines above', () => {
    // Enter pressed on line 10: 10..10 became 10..11.
    const e = [{ fromA: 10, toA: 10, fromB: 10, toB: 11 }];
    expect(mapLine(5, e)).toBe(5);
    expect(mapLine(10, e)).toBe(10);
    expect(mapLine(11, e)).toBe(12);
    expect(mapLine(400, e)).toBe(401);
  });

  it('clamps lines inside a deleted block to what replaced it', () => {
    // Lines 10..15 collapsed into line 10.
    const e = [{ fromA: 10, toA: 15, fromB: 10, toB: 10 }];
    expect(mapLine(12, e)).toBe(10);
    expect(mapLine(16, e)).toBe(11);
  });

  it('accumulates several changes', () => {
    // +2 lines at line 3, then −1 line at old line 20 (new 22..23 → 22..22).
    const e = [
      { fromA: 3, toA: 3, fromB: 3, toB: 5 },
      { fromA: 20, toA: 21, fromB: 22, toB: 22 },
    ];
    expect(mapLine(10, e)).toBe(12);
    expect(mapLine(30, e)).toBe(31);
  });

  it('moves stops in the edited file only, and merges stops an edit collapsed', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 40));
    h.leave(at('B.java', 40));
    h.leave(at('A.java', 42));
    h.leave(at('A.java', 43));
    h.applyEdits('A.java', [{ fromA: 1, toA: 1, fromB: 1, toB: 6 }]);
    expect(h.snapshot().back).toEqual([at('A.java', 45), at('B.java', 40), at('A.java', 47), at('A.java', 48)]);
    // Delete 47..48 into one line: the two adjacent stops become one.
    h.applyEdits('A.java', [{ fromA: 47, toA: 48, fromB: 47, toB: 47 }]);
    expect(h.snapshot().back).toEqual([at('A.java', 45), at('B.java', 40), at('A.java', 47)]);
  });

  it('builds line edits from a change set', () => {
    const text = 'a\nb\nc\n';
    const after = 'a\nX\nY\nb\nc\n';
    const doc = (s: string) => ({
      lineAt: (pos: number) => ({ number: s.slice(0, pos).split('\n').length }),
    });
    const changes = { iterChanges: (f: (a: number, b: number, c: number, d: number) => void) => f(2, 2, 2, 6) };
    expect(lineEditsOf(changes, doc(text), doc(after))).toEqual([{ fromA: 2, toA: 2, fromB: 2, toB: 4 }]);
  });
});

describe('files that are gone', () => {
  it('forget drops the file from both stacks and the recent list', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 1));
    h.leave(at('Gone.java', 1));
    h.leave(at('A.java', 1));
    h.forget('Gone.java');
    // The two A:1 stops the removal made adjacent are one.
    expect(h.snapshot().back).toEqual([at('A.java', 1)]);
    expect(h.recent.some((r) => r.file === 'Gone.java')).toBe(false);
  });
});

describe('edits and recent places', () => {
  it('merges a burst of typing and walks edits newest first', () => {
    const h = new NavHistory();
    h.noteEdit(at('A.java', 10));
    h.noteEdit(at('A.java', 12));
    h.noteEdit(at('B.java', 3));
    expect(h.stepEdit()).toEqual(at('B.java', 3));
    expect(h.stepEdit()).toEqual(at('A.java', 12));
    expect(h.stepEdit()).toBeNull();
  });

  it('reset clears everything', () => {
    const h = new NavHistory();
    h.leave(at('A.java', 1));
    h.noteEdit(at('A.java', 1));
    h.reset();
    expect(h.canBack || h.canForward || h.hasEdits || h.recent.length > 0).toBe(false);
  });
});
