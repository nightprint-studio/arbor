import { describe, expect, it } from 'vitest';
import { treeActionsFor, treeDraft, treePrompt } from './tree-actions';
import { groupedMenuItems } from './menu-groups';

describe('tree actions', () => {
  it('offers a file the things that make sense for a file, with the reviews grouped', () => {
    const items = groupedMenuItems(treeActionsFor(false));
    expect(items.map((i) => i.label)).toEqual(['Explain this file', 'Review', 'Write tests for it', 'Ask about this…']);
    expect(items[1].children?.map((c) => c.id)).toEqual(['bugs', 'improve']);
  });

  it('offers a folder an overview, its problems and a free question', () => {
    expect(treeActionsFor(true).map((a) => a.id)).toEqual(['explain', 'problems', 'ask']);
  });

  it('names the path in the request and says the review changes nothing', () => {
    expect(treePrompt('explain', 'src/A.java', false)).toContain('`src/A.java`');
    expect(treePrompt('explain', 'src/orders', true)).toContain('folder `src/orders`');
    expect(treePrompt('bugs', 'src/A.java', false)).toContain('change nothing yet');
  });

  it('starts a free question with the path already in it', () => {
    expect(treeDraft('src/A.java')).toBe('About `src/A.java`: ');
  });
});
