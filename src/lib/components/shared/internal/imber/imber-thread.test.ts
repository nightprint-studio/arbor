import { describe, expect, it } from 'vitest';
import { groupConversations, isNearBottom } from './imber-thread';

describe('isNearBottom', () => {
  it('follows when the reader is within the slack of the end', () => {
    expect(isNearBottom(1000, 700, 250)).toBe(true);
    expect(isNearBottom(1000, 100, 250)).toBe(false);
  });
});

describe('groupConversations', () => {
  const now = 10 * 24 * 3600_000;
  const c = (id: string, ageH: number) => ({ id, title: id, updatedAt: now - ageH * 3600_000 });
  it('puts the last 24 hours under recent and keeps the order given', () => {
    const { recent, earlier } = groupConversations([c('a', 1), c('b', 23), c('c', 25), c('d', 100)], now);
    expect(recent.map((x) => x.id)).toEqual(['a', 'b']);
    expect(earlier.map((x) => x.id)).toEqual(['c', 'd']);
  });
});
