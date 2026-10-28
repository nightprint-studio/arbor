import { describe, expect, it } from 'vitest';
import {
  clampOffset,
  insertBefore,
  landingIndex,
  settleOffset,
  shiftDirection,
  type TabSlot,
} from './tabs-sort';

/** Four 100px tabs a 2px gap apart: A B C D at 0, 102, 204, 306. */
const slots: TabSlot[] = [0, 1, 2, 3].map((index) => ({ index, left: index * 102, width: 100 }));

describe('insertBefore', () => {
  it('stays put until the centre passes a neighbour', () => {
    // B (index 1) centre is 152; dragging it a little either way keeps it where it is — which is
    // always spelled "before the tab after it".
    expect(insertBefore(slots, 1, 152)).toBe(2);
    expect(insertBefore(slots, 1, 100)).toBe(2);
    expect(insertBefore(slots, 1, 200)).toBe(2);
  });

  it('lands before the first tab whose centre it passed', () => {
    // A's centre is 50: dragging B left of it goes before A.
    expect(insertBefore(slots, 1, 40)).toBe(0);
    // Past C's centre (254) but not D's (356): before D.
    expect(insertBefore(slots, 1, 300)).toBe(3);
  });

  it('goes to the end past the last centre', () => {
    expect(insertBefore(slots, 0, 500)).toBe(4);
  });

  it('counts hidden tabs through the indices, not through the position among the visible', () => {
    // Tabs 0..2 visible, 3..5 behind the chevron: the end of the visible strip is before index 3.
    expect(insertBefore(slots.slice(0, 3), 0, 500)).toBe(3);
  });
});

describe('landingIndex', () => {
  it('moves right by one less than the insert point, left exactly there', () => {
    expect(landingIndex(1, 4)).toBe(3);
    expect(landingIndex(3, 1)).toBe(1);
    expect(landingIndex(2, 2)).toBe(2);
    expect(landingIndex(2, 3)).toBe(2);
  });
});

describe('shiftDirection', () => {
  it('slides the tabs it passes toward where the dragged one came from', () => {
    // B dragged to before D: C slides left; A and D stay.
    expect([0, 1, 2, 3].map((i) => shiftDirection(i, 1, 3))).toEqual([0, 0, -1, 0]);
    // D dragged to before B: B and C slide right.
    expect([0, 1, 2, 3].map((i) => shiftDirection(i, 3, 1))).toEqual([0, 1, 1, 0]);
  });

  it('moves nothing while the tab has not passed a neighbour', () => {
    expect([0, 1, 2, 3].map((i) => shiftDirection(i, 1, 1))).toEqual([0, 0, 0, 0]);
    expect([0, 1, 2, 3].map((i) => shiftDirection(i, 1, 2))).toEqual([0, 0, 0, 0]);
  });
});

describe('settleOffset', () => {
  it('is the width of what it jumped, gaps included', () => {
    expect(settleOffset(slots, 1, 3, 2)).toBe(102); // over C
    expect(settleOffset(slots, 0, 4, 2)).toBe(306); // over B, C, D
    expect(settleOffset(slots, 3, 1, 2)).toBe(-204); // back over B and C
  });

  it('is zero when it did not move', () => {
    expect(settleOffset(slots, 2, 2, 2)).toBe(0);
    expect(settleOffset(slots, 2, 3, 2)).toBe(0);
  });
});

describe('clampOffset', () => {
  it('keeps the tab inside the strip', () => {
    expect(clampOffset(slots, 1, -500)).toBe(-102);
    expect(clampOffset(slots, 1, 500)).toBe(204);
    expect(clampOffset(slots, 1, 30)).toBe(30);
  });

  it('is zero for a tab that is not in the strip', () => {
    expect(clampOffset(slots, 9, 50)).toBe(0);
  });
});
