import { describe, expect, it } from 'vitest';
import type { BrpRow } from '$lib/ipc/bennu/brp';
import { branchIds, buildFilterTree, isFilterKey } from './brp-tree';

const row = (id: number, parent: number | null, name = `e${id}`): BrpRow => ({ id, name, parent, child_count: 0 });

describe('buildFilterTree', () => {
  it('nests hits under the ancestors they came with', () => {
    const tree = buildFilterTree([row(3, 2), row(1, null), row(2, 1)], [1, 2]);
    expect(tree).toHaveLength(1);
    expect(tree[0].row.id).toBe(1);
    expect(tree[0].hit).toBe(false);
    expect(tree[0].children[0].children[0].row.id).toBe(3);
    expect(tree[0].children[0].children[0].hit).toBe(true);
  });

  it('makes a row whose parent is missing a root, ordered by id', () => {
    const tree = buildFilterTree([row(9, 100), row(4, null)]);
    expect(tree.map((n) => n.row.id)).toEqual([4, 9]);
    expect(tree.every((n) => n.hit)).toBe(true);
  });

  it('lists the branches to open', () => {
    const tree = buildFilterTree([row(1, null), row(2, 1), row(3, 2), row(4, null)]);
    expect(branchIds(tree)).toEqual([1, 2]);
  });
});

describe('isFilterKey', () => {
  const k = (key: string, mods: Partial<KeyboardEvent> = {}) => isFilterKey({ key, ctrlKey: false, metaKey: false, altKey: false, ...mods });
  it('takes a letter, not a shortcut, space or a named key', () => {
    expect(k('g')).toBe(true);
    expect(k('g', { ctrlKey: true })).toBe(false);
    expect(k(' ')).toBe(false);
    expect(k('Enter')).toBe(false);
    expect(k('ArrowDown')).toBe(false);
  });
});
