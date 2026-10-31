import { describe, expect, it } from 'vitest';
import { stableLineNumberWidth } from './theme';

describe('stableLineNumberWidth', () => {
  it('reserves four digits and the cell padding by default', () => {
    expect(stableLineNumberWidth()).toBe('calc(4ch + 22px)');
  });

  it('never reserves fewer than two digits', () => {
    expect(stableLineNumberWidth(0)).toBe('calc(2ch + 22px)');
    expect(stableLineNumberWidth(5.9)).toBe('calc(5ch + 22px)');
  });
});
