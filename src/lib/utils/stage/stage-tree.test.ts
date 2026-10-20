import { describe, expect, it } from 'vitest';
import type { StatusEntry } from '$lib/types/corvus/git';
import { buildStageTree, collectDirs, folderTarget, reconcileExpanded } from './stage-tree';

const entry = (path: string, old_path: string | null = null): StatusEntry =>
  ({ path, old_path, index_status: null, workdir_status: 'modified' }) as unknown as StatusEntry;

describe('buildStageTree', () => {
  it('lists folders before files, each alphabetically', () => {
    const tree = buildStageTree([entry('z.txt'), entry('src/b.rs'), entry('a.txt'), entry('src/a.rs')]);
    expect(tree.sortedChildren.map((n) => n.name)).toEqual(['src', 'a.txt', 'z.txt']);
    expect(tree.sortedChildren[0].sortedChildren.map((n) => n.name)).toEqual(['a.rs', 'b.rs']);
  });

  it('collapses a single-child chain only when compacting', () => {
    const entries = [entry('a/b/c/x.rs'), entry('a/b/c/y.rs')];
    expect(buildStageTree(entries).sortedChildren[0].name).toBe('a');
    expect(buildStageTree(entries, true).sortedChildren[0].name).toBe('a/b/c');
  });
});

describe('folderTarget', () => {
  it('counts a rename as one file but sends both paths', () => {
    const tree = buildStageTree([entry('d/new.rs', 'd/old.rs'), entry('d/other.rs')]);
    const t = folderTarget(tree.sortedChildren[0]);
    expect(t.count).toBe(2);
    expect(t.paths.sort()).toEqual(['d/new.rs', 'd/old.rs', 'd/other.rs']);
    expect(t.label).toBe('2 files in d');
  });
});

describe('reconcileExpanded', () => {
  it('opens new folders, keeps a collapsed one collapsed and forgets the gone', () => {
    const next = reconcileExpanded(
      new Set(['kept', 'collapsed', 'fresh']),
      new Set(['kept', 'gone']),
      new Set(['kept', 'collapsed', 'gone']),
    );
    expect([...next].sort()).toEqual(['fresh', 'kept']);
  });

  it('reads folders from the rendered tree', () => {
    expect([...collectDirs(buildStageTree([entry('a/x.rs')]))]).toEqual(['a']);
  });
});
