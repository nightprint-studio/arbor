import { describe, expect, it } from 'vitest';
import { findNode, graftChildren, hasUnloaded, nextUnloadedOnTheWay, openAndUnlisted } from './tree-graft';
import type { TreeNode } from '$lib/types/bennu';

const dir = (path: string, children: TreeNode[] = [], unloaded = false): TreeNode => ({ name: path.split('/').pop()!, path, is_dir: true, children, unloaded });
const file = (path: string): TreeNode => ({ name: path.split('/').pop()!, path, is_dir: false, children: [] });

const tree = () => dir('/p', [dir('/p/src', [dir('/p/src/main', [], true)]), dir('/p/docs', [], true), file('/p/pom.xml')]);

describe('a tree fetched a folder at a time', () => {
  it('finds a node by path, whatever the slashes', () => {
    expect(findNode(tree(), '/p/src/main')?.unloaded).toBe(true);
    expect(findNode(tree(), '\\p\\src')?.name).toBe('src');
    expect(findNode(tree(), '/p/nope')).toBeNull();
  });

  it('grafts a fetched folder in and keeps every other node as it was', () => {
    const before = tree();
    const fresh = dir('/p/docs', [file('/p/docs/a.md'), file('/p/docs/b.md')]);
    const after = graftChildren(before, '/p/docs', fresh);
    const docs = findNode(after, '/p/docs')!;
    expect(docs.children.map((c) => c.name)).toEqual(['a.md', 'b.md']);
    expect(docs.unloaded).toBe(false);
    // the branch that did not change is the same object, so nothing that draws from it redraws
    expect(findNode(after, '/p/src')).toBe(findNode(before, '/p/src'));
    // and the original is untouched
    expect(findNode(before, '/p/docs')!.unloaded).toBe(true);
  });

  it('answers the same tree for a folder that is not in it', () => {
    const before = tree();
    expect(graftChildren(before, '/p/gone', dir('/p/gone', [file('/p/gone/x')]))).toBe(before);
  });

  it('knows whether anything is left to fetch', () => {
    expect(hasUnloaded(tree())).toBe(true);
    const done = graftChildren(graftChildren(tree(), '/p/docs', dir('/p/docs')), '/p/src/main', dir('/p/src/main'));
    expect(hasUnloaded(done)).toBe(false);
  });

  it('names the folder to fetch next on the way to a path, one at a time', () => {
    expect(nextUnloadedOnTheWay(tree(), '/p/src/main/java/App.java')).toBe('/p/src/main');
    const opened = graftChildren(tree(), '/p/src/main', dir('/p/src/main', [dir('/p/src/main/java', [], true)]));
    expect(nextUnloadedOnTheWay(opened, '/p/src/main/java/App.java')).toBe('/p/src/main/java');
    expect(nextUnloadedOnTheWay(tree(), '/p/pom.xml')).toBeNull();
    expect(nextUnloadedOnTheWay(tree(), '/p/elsewhere/x')).toBeNull();
  });

  it('lists the open folders that still have to be fetched, shallowest first', () => {
    expect(openAndUnlisted(tree(), ['/p/src/main', '/p/docs', '/p/src', '/p/missing'])).toEqual(['/p/docs', '/p/src/main']);
  });
});
