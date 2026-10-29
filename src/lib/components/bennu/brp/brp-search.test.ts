import { describe, expect, it } from 'vitest';
import { isEmptySearch, parseSearch } from './brp-search';

describe('brp-search', () => {
  it('splits filters from free text', () => {
    expect(parseSearch('goblin has:Health no:Dead')).toEqual({ text: 'goblin', has: ['Health'], no: ['Dead'] });
  });

  it('accepts the long spellings and full paths', () => {
    const p = parseSearch('with:my_game::Health without:Dead');
    expect(p.has).toEqual(['my_game::Health']);
    expect(p.no).toEqual(['Dead']);
  });

  it('keeps several words of free text together', () => {
    expect(parseSearch('  big   goblin ').text).toBe('big goblin');
  });

  it('knows when there is nothing to filter by', () => {
    expect(isEmptySearch(parseSearch('   '))).toBe(true);
    expect(isEmptySearch(parseSearch('has:Name'))).toBe(false);
  });
});
