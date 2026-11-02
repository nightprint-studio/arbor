import { describe, expect, it } from 'vitest';
import type { RecentProject } from '$lib/ipc/recents';
import { ageGroup, greetingFor, promptPhrases, recentsView, searchEntries, whenLabel } from './welcome-logic';

const NOW = 1_800_000_000, DAY = 86_400;
const r = (name: string, product: string, daysAgo: number, pinned = false): RecentProject =>
  ({ product, path: `/p/${name}`, name, opened_at: NOW - daysAgo * DAY, pinned });

describe('greeting and prompt', () => {
  it('changes at 5, 12 and 18', () => {
    expect([0, 4, 5, 11, 12, 17, 18, 23].map(greetingFor)).toEqual([
      'Still up', 'Still up', 'Good morning', 'Good morning', 'Good afternoon', 'Good afternoon', 'Good evening', 'Good evening',
    ]);
  });
  it('starts with the last project when there is one', () => {
    expect(promptPhrases('orders')[0]).toBe('Continue with orders');
    expect(promptPhrases(null)[0]).toBe('Open a project, a repository or a folder');
  });
});

describe('time', () => {
  it('groups by age', () => {
    expect([0.2, 1.5, 3, 12, 40].map((d) => ageGroup(NOW - d * DAY, NOW))).toEqual(['Today', 'Yesterday', 'This week', 'This month', 'Earlier']);
  });
  it('labels a day count', () => {
    expect(whenLabel(NOW - 100, NOW)).toBe('today');
    expect(whenLabel(NOW - DAY, NOW)).toBe('yesterday');
    expect(whenLabel(NOW - 3 * DAY, NOW)).toBe('3d ago');
    expect(whenLabel(NOW - 15 * DAY, NOW)).toBe('2w ago');
  });
});

describe('recents view', () => {
  const all = [r('a', 'bennu', 0.1), r('b', 'corvus', 0.5, true), r('c', 'bennu', 1.2), r('d', 'merula', 3), r('e', 'bennu', 9), r('f', 'corvus', 40)];

  it('puts pinned first and groups the rest by age, newest first', () => {
    const v = recentsView(all, 'all', NOW, { all: true, limit: 6 });
    expect(v.pinned.map((x) => x.name)).toEqual(['b']);
    expect(v.groups.map((g) => [g.label, g.items.map((x) => x.name)])).toEqual([
      ['Today', ['a']], ['Yesterday', ['c']], ['This week', ['d']], ['This month', ['e']], ['Earlier', ['f']],
    ]);
    expect(v.order.map((x) => x.name)).toEqual(['b', 'a', 'c', 'd', 'e', 'f']);
  });

  it('filters by product and keeps the pinned of that product only', () => {
    const v = recentsView(all, 'corvus', NOW, { all: true, limit: 6 });
    expect(v.order.map((x) => x.name)).toEqual(['b', 'f']);
  });

  it('caps what is shown and says how many are hidden', () => {
    const v = recentsView(all, 'all', NOW, { all: false, limit: 2 });
    expect(v.order.map((x) => x.name)).toEqual(['b', 'a', 'c']);
    expect(v.hidden).toBe(3);
  });
});

describe('search', () => {
  const entries = [
    { id: 'p1', title: 'orders-service', detail: '~/work/shop/orders-service', kind: 'project' as const },
    { id: 'p2', title: 'arbor', detail: '~/dev/arbor', kind: 'project' as const },
    { id: 'bennu', title: 'Bennu', detail: 'Java editor', kind: 'product' as const },
    { id: 'clone', title: 'Clone repository…', detail: '', kind: 'action' as const },
  ];
  it('lists everything for an empty query', () => {
    expect(searchEntries('', entries).map((g) => g.label)).toEqual(['Projects', 'Products', 'Actions']);
  });
  it('matches a project by its path too, and drops empty groups', () => {
    expect(searchEntries('shop', entries)).toEqual([{ label: 'Projects', items: [entries[0]] }]);
  });
  it('is case-insensitive', () => {
    expect(searchEntries('CLONE', entries)).toEqual([{ label: 'Actions', items: [entries[3]] }]);
  });
});
