import { describe, it, expect } from 'vitest';

import { applyRailOrder, mergeRailOrder } from './rail-order';

const item = (id: string) => ({ id });
const ids = (xs: { id: string }[]) => xs.map((x) => x.id);

const DEFAULT = ['maven', 'gradle', 'cargo', 'node', 'tests', 'bevy', 'ast', 'http', 'assistant'].map(item);

describe('applyRailOrder', () => {
  it('is the default order when nothing was saved', () => {
    expect(ids(applyRailOrder(DEFAULT, undefined))).toEqual(ids(DEFAULT));
    expect(ids(applyRailOrder(DEFAULT, []))).toEqual(ids(DEFAULT));
  });

  it('keeps the user\'s order and hides what they hid', () => {
    const saved = [
      { id: 'assistant', visible: true },
      { id: 'maven', visible: true },
      { id: 'tests', visible: false },
    ];
    const out = ids(applyRailOrder(DEFAULT, saved));
    expect(out.slice(0, 2)).toEqual(['assistant', 'maven']);
    expect(out).not.toContain('tests');
  });

  it('puts a button the saved layout never heard of at its default place, not at the end', () => {
    // Saved before Gradle and Cargo existed on this rail.
    const saved = ['maven', 'node', 'tests', 'http', 'assistant'].map((id) => ({ id, visible: true }));
    const out = ids(applyRailOrder(DEFAULT, saved));
    expect(out.indexOf('gradle')).toBe(out.indexOf('maven') + 1);
    expect(out.indexOf('cargo')).toBe(out.indexOf('gradle') + 1);
    expect(out.indexOf('node')).toBe(out.indexOf('cargo') + 1);
    expect(out.indexOf('bevy')).toBe(out.indexOf('tests') + 1);
  });

  it('places a new first button before the first one that follows it', () => {
    const items = ['a', 'b', 'c'].map(item);
    const out = ids(applyRailOrder(items, [{ id: 'b', visible: true }, { id: 'c', visible: true }]));
    expect(out).toEqual(['a', 'b', 'c']);
  });

  it('lets a hidden neighbour mark the place', () => {
    const items = ['a', 'b', 'c'].map(item);
    const out = ids(applyRailOrder(items, [{ id: 'a', visible: false }, { id: 'c', visible: true }]));
    // `b` is new: it follows `a` (hidden) and so sits before `c`.
    expect(out).toEqual(['b', 'c']);
  });

  it('keeps a mandatory item visible', () => {
    const out = ids(applyRailOrder(DEFAULT, [{ id: 'maven', visible: false }], new Set(['maven'])));
    expect(out).toContain('maven');
  });
});

describe('mergeRailOrder', () => {
  it('gives the new button a row at its default place, visible', () => {
    const saved = [{ id: 'maven', visible: true }, { id: 'tests', visible: false }];
    const merged = mergeRailOrder(DEFAULT, saved);
    const order = merged.map((m) => m.id);
    expect(order.indexOf('gradle')).toBe(order.indexOf('maven') + 1);
    expect(merged.find((m) => m.id === 'gradle')?.visible).toBe(true);
    expect(merged.find((m) => m.id === 'tests')?.visible).toBe(false);
  });
});
