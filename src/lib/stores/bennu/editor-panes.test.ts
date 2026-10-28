import { describe, expect, it } from 'vitest';
import {
  activate, activeOf, allTabs, close, closeAll, closeOthers, closeToRight, emptyPanes, focus, fromStored, fromTabs, isSplit,
  merge, moveTo, normalize, open, paneOf, reorder, replacePath, toStored, type Panes,
} from './editor-panes';

const opened = (...paths: string[]): Panes => paths.reduce((p, path) => open(p, path), emptyPanes);
/** A B C in the first pane, D E in the second; A active in the first and E in the second, first focused. */
const split = (): Panes =>
  focus(
    moveTo(moveTo(opened('A', 'B', 'C', 'D', 'E'), 'D', 1), 'E', 1),
    0,
  );

describe('opening', () => {
  it('opens at the end of the focused pane and makes it active', () => {
    const p = opened('A', 'B');
    expect(p.tabs[0]).toEqual(['A', 'B']);
    expect(activeOf(p)).toBe('B');
    expect(isSplit(p)).toBe(false);
  });

  it('a file already open is activated where it is, focusing that pane', () => {
    const p = open(focus(split(), 0), 'D');
    expect(paneOf(p, 'D')).toBe(1);
    expect(p.focused).toBe(1);
    expect(activeOf(p)).toBe('D');
    expect(allTabs(p).filter((t) => t === 'D')).toHaveLength(1);
  });

  it('a new file goes to the pane that has the focus', () => {
    const p = open(activate(split(), 'E'), 'F');
    expect(p.tabs[1]).toEqual(['D', 'E', 'F']);
    expect(p.tabs[0]).toEqual(['A', 'B', 'C']);
  });
});

describe('moving between panes', () => {
  it('moving a tab to the second pane creates the split and focuses it', () => {
    const p = moveTo(opened('A', 'B', 'C'), 'B', 1);
    expect(p.tabs).toEqual([['A', 'C'], ['B']]);
    expect(p.focused).toBe(1);
    expect(activeOf(p)).toBe('B');
    expect(isSplit(p)).toBe(true);
  });

  it('the pane it left keeps a valid active tab', () => {
    const p = moveTo(opened('A', 'B', 'C'), 'C', 1); // C was the first pane's active tab
    expect(p.active[0]).toBe('A');
  });

  it('can land at a position', () => {
    const p = moveTo(split(), 'A', 1, 1);
    expect(p.tabs[1]).toEqual(['D', 'A', 'E']);
  });

  it('the only tab of a pane cannot be moved into a split of its own', () => {
    const p = moveTo(opened('A'), 'A', 1);
    expect(p.tabs).toEqual([['A'], []]);
    expect(isSplit(p)).toBe(false);
  });

  it('moving the last tab out of the second pane collapses the split', () => {
    let p = moveTo(opened('A', 'B'), 'B', 1);
    p = moveTo(p, 'B', 0);
    expect(p.tabs).toEqual([['A', 'B'], []]);
    expect(p.focused).toBe(0);
    expect(p.active[1]).toBeNull();
  });

  it('a path that is not open is ignored', () => {
    const p = opened('A', 'B');
    expect(moveTo(p, 'Z', 1)).toBe(p);
  });
});

describe('closing', () => {
  it('the neighbour in the same pane takes over', () => {
    const p = close(activate(split(), 'B'), 'B');
    expect(p.active[0]).toBe('C');
    expect(p.tabs[1]).toEqual(['D', 'E']);
  });

  it('closing the last tab of the second pane collapses the split', () => {
    let p = split();
    p = close(close(p, 'D'), 'E');
    expect(isSplit(p)).toBe(false);
    expect(p.focused).toBe(0);
  });

  it('closing the last tab of the first pane promotes the second into its place', () => {
    let p = split();
    p = close(close(close(p, 'A'), 'B'), 'C');
    expect(p.tabs).toEqual([['D', 'E'], []]);
    expect(activeOf(p)).toBe('E'); // the second pane's own active tab comes with it
    expect(p.focused).toBe(0);
  });

  it('closing in an unfocused pane leaves the focus alone', () => {
    const p = close(split(), 'D');
    expect(p.focused).toBe(0);
    expect(p.active[1]).toBe('E');
  });

  it('close others leaves one tab in one pane; close all leaves nothing', () => {
    expect(closeOthers(split(), 'D').tabs).toEqual([['D'], []]);
    expect(allTabs(closeAll())).toEqual([]);
    expect(activeOf(closeAll())).toBeNull();
  });

  it('close to the right stays in the tab\'s own pane', () => {
    const p = closeToRight(split(), 'D');
    expect(p.tabs).toEqual([['A', 'B', 'C'], ['D']]);
    expect(closeToRight(p, 'A').tabs[0]).toEqual(['A']);
  });
});

describe('merging', () => {
  it('puts the second pane\'s tabs after the first\'s and keeps the edited file active', () => {
    const p = merge(activate(split(), 'E'));
    expect(p.tabs).toEqual([['A', 'B', 'C', 'D', 'E'], []]);
    expect(activeOf(p)).toBe('E');
    expect(isSplit(p)).toBe(false);
  });

  it('is nothing when there is no split', () => {
    const p = opened('A', 'B');
    expect(merge(p)).toBe(p);
  });
});

describe('reorder and rename', () => {
  it('reorders within one pane only', () => {
    const p = reorder(split(), 0, 0, 2);
    expect(p.tabs[0]).toEqual(['B', 'C', 'A']);
    expect(p.tabs[1]).toEqual(['D', 'E']);
  });

  it('an out-of-range or no-op reorder changes nothing', () => {
    const p = split();
    expect(reorder(p, 0, 5, 0)).toBe(p);
    expect(reorder(p, 0, 1, 1)).toBe(p);
  });

  it('a renamed file keeps its pane, place and active state', () => {
    const p = replacePath(activate(split(), 'E'), 'E', 'E2');
    expect(p.tabs[1]).toEqual(['D', 'E2']);
    expect(p.active[1]).toBe('E2');
  });
});

describe('stored sessions', () => {
  it('a split survives the round trip, with each pane\'s active tab and the focus', () => {
    const p = activate(split(), 'D'); // second pane focused, D active there; A active in the first
    const back = fromStored(toStored(p));
    expect(back.tabs).toEqual(p.tabs);
    expect(back.active).toEqual(p.active);
    expect(back.focused).toBe(1);
  });

  it('the whole tab list stays whole, so an older build opens every tab', () => {
    expect(toStored(split()).open_files).toEqual(['A', 'B', 'C', 'D', 'E']);
  });

  it('an unsplit editor stores no second pane', () => {
    const stored = toStored(opened('A', 'B'));
    expect(stored.split_files).toEqual([]);
    expect(stored.first_active).toBe('');
    expect(stored.split_active).toBe('');
  });

  it('a session from before the split existed is one pane', () => {
    const p = fromStored({ open_files: ['A', 'B', 'C'], active_file: 'B' });
    expect(p.tabs).toEqual([['A', 'B', 'C'], []]);
    expect(activeOf(p)).toBe('B');
  });

  it('a split tab that is no longer in the list, or a split of nothing, collapses to one pane', () => {
    expect(fromStored({ open_files: ['A', 'B'], split_files: ['Z'], active_file: 'A' }).tabs).toEqual([['A', 'B'], []]);
    expect(isSplit(fromStored({ open_files: ['A'], split_files: ['A'], active_file: 'A' }))).toBe(false);
  });

  it('the focus is wherever the stored active file is', () => {
    const p = fromStored({ open_files: ['A', 'B', 'C'], split_files: ['C'], active_file: 'C', first_active: 'A', split_active: 'C' });
    expect(p.focused).toBe(1);
    expect(p.active).toEqual(['A', 'C']);
  });
});

describe('invariants', () => {
  it('a file in both panes stays in the first', () => {
    const p = normalize({ tabs: [['A'], ['A', 'B']], active: ['A', 'A'], focused: 1 });
    expect(p.tabs).toEqual([['A'], ['B']]);
    expect(p.active[1]).toBe('B');
  });

  it('a focus on a pane that has no tabs falls back to the first', () => {
    expect(focus(opened('A'), 1).focused).toBe(0);
  });

  it('a session that knows no split restores to one pane', () => {
    const p = fromTabs(['A', 'B', 'C'], 'B');
    expect(p.tabs[0]).toEqual(['A', 'B', 'C']);
    expect(activeOf(p)).toBe('B');
    expect(fromTabs(['A'], 'gone').active[0]).toBe('A');
  });
});
