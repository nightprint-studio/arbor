import { describe, expect, it } from 'vitest';
import { groupedMenuItems } from './menu-groups';

describe('groupedMenuItems', () => {
  it('puts the actions of one group under a single parent, where the first of them was', () => {
    const items = groupedMenuItems([
      { id: 'improve', label: 'Improve this comment', group: 'Improve', child: 'Clearer wording' },
      { id: 'review', label: 'Check against the code' },
      { id: 'shorten', label: 'Make it shorter', group: 'Improve', child: 'Shorter' },
      { id: 'en', label: 'Translate to English', group: 'Translate', child: 'English' },
      { id: 'it', label: 'Translate to Italian', group: 'Translate', child: 'Italian' },
    ]);
    expect(items.map((i) => i.label)).toEqual(['Improve', 'Check against the code', 'Translate']);
    expect(items[0].children?.map((c) => [c.id, c.label])).toEqual([['improve', 'Clearer wording'], ['shorten', 'Shorter']]);
    expect(items[2].children?.map((c) => c.id)).toEqual(['en', 'it']);
  });

  it('keeps a group of one as a plain item with its full label', () => {
    const items = groupedMenuItems([
      { id: 'tests', label: 'Write tests for it', group: 'Write', child: 'Tests' },
      { id: 'ask', label: 'Ask about this…' },
    ]);
    expect(items).toHaveLength(2);
    expect(items[0]).toMatchObject({ id: 'tests', label: 'Write tests for it' });
    expect(items[0].children).toBeUndefined();
  });

  it('leaves ungrouped lists as they are', () => {
    const items = groupedMenuItems([{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }]);
    expect(items.map((i) => i.id)).toEqual(['a', 'b']);
  });
});
