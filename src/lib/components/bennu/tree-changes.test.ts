import { describe, it, expect } from 'vitest';
import type { TreeChange, TreeChanged } from '$lib/ipc/bennu/tree-watch';
import type { TreeNode } from '$lib/types/bennu';
import {
  describeNotice,
  findNode,
  isEmptyNotice,
  listingDiffers,
  manifestChanged,
  mergeNotices,
  needsTreeReload,
  reconcileTargets,
  renamedExpansion,
  structureNotice,
} from './tree-changes';

function payload(over: Partial<TreeChanged> = {}): TreeChanged {
  return { root: 'C:/w/app', paths: [], truncated: false, structural: true, rescan: false, changes: [], ...over };
}

function dir(path: string, children: TreeNode[] = []): TreeNode {
  return { name: path.split('/').pop() ?? path, path, is_dir: true, children };
}

function file(path: string): TreeNode {
  return { name: path.split('/').pop() ?? path, path, is_dir: false, children: [] };
}

describe('needsTreeReload', () => {
  it('reloads for a structural change or a possible loss', () => {
    expect(needsTreeReload(payload())).toBe(true);
    expect(needsTreeReload(payload({ structural: false, rescan: true }))).toBe(true);
    expect(needsTreeReload(payload({ structural: false, truncated: true }))).toBe(true);
  });

  it('does not reload for a manifest edit, but does for a .gitignore edit', () => {
    expect(needsTreeReload(payload({ structural: false, paths: ['pom.xml'] }))).toBe(false);
    expect(needsTreeReload(payload({ structural: false, paths: ['core/.gitignore'] }))).toBe(true);
  });
});

describe('manifestChanged', () => {
  it('sees a manifest at any depth, and assumes one when the list is incomplete', () => {
    expect(manifestChanged(payload({ paths: ['core/pom.xml'] }))).toBe(true);
    expect(manifestChanged(payload({ paths: ['crates/a/Cargo.toml'] }))).toBe(true);
    expect(manifestChanged(payload({ paths: ['src/Main.java'] }))).toBe(false);
    expect(manifestChanged(payload({ rescan: true }))).toBe(true);
  });
});

describe('structureNotice', () => {
  it('notifies for a folder added or removed under the root', () => {
    const n = structureNotice(payload({
      changes: [
        { kind: 'added', path: 'docs', is_dir: true, module: false },
        { kind: 'removed', path: 'legacy', is_dir: true },
      ],
    }));
    expect(n).toEqual({ added: ['docs'], removed: ['legacy'], renamed: [], rebuildIndex: false });
  });

  it('stays quiet for files and for nested plain folders', () => {
    expect(structureNotice(payload({
      changes: [
        { kind: 'added', path: 'README.md', is_dir: false, module: false },
        { kind: 'added', path: 'src/main/java/it/acme', is_dir: true, module: false },
        { kind: 'removed', path: 'src/old', is_dir: true },
      ],
    }))).toBeNull();
  });

  it('offers a rebuild for a module renamed anywhere', () => {
    const n = structureNotice(payload({
      changes: [{ kind: 'renamed', from: 'modules/invoices', path: 'modules/billing', is_dir: true, module: true }],
    }));
    expect(n?.renamed).toEqual([{ from: 'modules/invoices', to: 'modules/billing' }]);
    expect(n?.rebuildIndex).toBe(true);
  });

  it('offers a rebuild when a pom changed alongside a structural change', () => {
    const n = structureNotice(payload({
      paths: ['legacy', 'pom.xml'],
      changes: [{ kind: 'removed', path: 'legacy', is_dir: true }],
    }));
    expect(n?.rebuildIndex).toBe(true);
  });
});

describe('mergeNotices', () => {
  const empty = { added: [], removed: [], renamed: [], rebuildIndex: false };

  it('dedupes, chains renames and cancels an entry that came and went', () => {
    const a = { ...empty, added: ['tmp', 'docs'], renamed: [{ from: 'a', to: 'b' }] };
    const b = { ...empty, removed: ['tmp'], renamed: [{ from: 'b', to: 'c' }], added: ['docs'], rebuildIndex: true };
    expect(mergeNotices(a, b)).toEqual({
      added: ['docs'],
      removed: [],
      renamed: [{ from: 'a', to: 'c' }],
      rebuildIndex: true,
    });
  });

  it('a rename there and back is nothing to say', () => {
    const merged = mergeNotices({ ...empty, renamed: [{ from: 'a', to: 'b' }] }, { ...empty, renamed: [{ from: 'b', to: 'a' }] });
    expect(isEmptyNotice(merged)).toBe(true);
  });
});

describe('describeNotice', () => {
  it('reads as one line and caps long lists', () => {
    expect(describeNotice({ added: ['foo'], removed: [], renamed: [{ from: 'a', to: 'b' }], rebuildIndex: false }))
      .toBe("Project structure changed: added 'foo', renamed 'a' → 'b'");
    expect(describeNotice({ added: ['a', 'b', 'c', 'd', 'e'], removed: [], renamed: [], rebuildIndex: false }, 'shop'))
      .toBe("Project structure changed in shop: added 'a', 'b', 'c' and 2 more");
  });
});

describe('renamedExpansion', () => {
  it('moves the renamed folder and everything open inside it, and nothing else', () => {
    const changes: TreeChange[] = [{ kind: 'renamed', from: 'core', path: 'kernel', is_dir: true, module: false }];
    const ids = ['C:/w/app/core', 'C:/w/app/core/src', 'C:/w/app/core-extra', 'C:/w/app/docs'];
    expect(renamedExpansion(ids, 'C:\\w\\app', changes)).toEqual([
      ['C:/w/app/core', 'C:/w/app/kernel'],
      ['C:/w/app/core/src', 'C:/w/app/kernel/src'],
    ]);
  });
});

describe('reconciliation', () => {
  it('re-lists the root first, then this project’s expanded folders, capped', () => {
    const expanded = ['C:/w/app/a', 'C:/w/other/b', 'C:/w/app/c', 'C:/w/app/d'];
    expect(reconcileTargets('C:/w/app/', expanded, 3)).toEqual(['C:/w/app', 'C:/w/app/a', 'C:/w/app/c']);
  });

  it('finds a node by path without matching a sibling that shares a prefix', () => {
    const tree = dir('C:/w/app', [dir('C:/w/app/core', [file('C:/w/app/core/pom.xml')]), dir('C:/w/app/core-extra')]);
    expect(findNode(tree, 'C:/w/app/core-extra')?.name).toBe('core-extra');
    expect(findNode(tree, 'C:/w/app/core/pom.xml')?.is_dir).toBe(false);
    expect(findNode(tree, 'C:/w/app/missing')).toBeUndefined();
  });

  it('a listing differs on a new, missing or retyped child — not on order', () => {
    const current = dir('C:/w/app', [dir('C:/w/app/a'), file('C:/w/app/b')]);
    expect(listingDiffers(current, dir('C:\\w\\app', [file('C:\\w\\app\\b'), dir('C:\\w\\app\\a')]))).toBe(false);
    expect(listingDiffers(current, dir('C:/w/app', [dir('C:/w/app/a'), file('C:/w/app/b'), dir('C:/w/app/new')]))).toBe(true);
    expect(listingDiffers(current, dir('C:/w/app', [dir('C:/w/app/a'), dir('C:/w/app/b')]))).toBe(true);
    expect(listingDiffers(undefined, dir('C:/w/app'))).toBe(true);
  });
});
