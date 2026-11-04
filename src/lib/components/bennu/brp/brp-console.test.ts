import { describe, expect, it } from 'vitest';
import { HISTORY_LIMIT, parseParams, pushHistory, suggestMethods } from './brp-console';

describe('parseParams', () => {
  it('treats an empty box as no params', () => {
    expect(parseParams('  ')).toEqual({ ok: true, value: undefined });
  });
  it('parses JSON and reports what is wrong with anything else', () => {
    expect(parseParams('{"entity": 4}')).toEqual({ ok: true, value: { entity: 4 } });
    const bad = parseParams('{entity: 4}');
    expect(bad.ok).toBe(false);
  });
});

describe('suggestMethods', () => {
  const methods = ['rpc.discover', 'world.query', 'world.get_components', 'my_game.query_wave', 'registry.schema'];
  it('offers nothing for an empty box', () => {
    expect(suggestMethods(methods, '')).toEqual([]);
  });
  it('puts prefix matches before substring matches', () => {
    expect(suggestMethods(methods, 'world')).toEqual(['world.get_components', 'world.query']);
    expect(suggestMethods(methods, 'query')).toEqual(['my_game.query_wave', 'world.query']);
    expect(suggestMethods([...methods, 'query_all'], 'query')[0]).toBe('query_all');
  });
  it('does not repeat the exact name already typed', () => {
    expect(suggestMethods(methods, 'world.query')).toEqual([]);
  });
});

describe('pushHistory', () => {
  it('moves a repeated call to the top', () => {
    const a = { method: 'a', params: '' };
    const b = { method: 'b', params: '' };
    expect(pushHistory([b, a], a)).toEqual([a, b]);
  });
  it('keeps the newest HISTORY_LIMIT', () => {
    let h: { method: string; params: string }[] = [];
    for (let i = 0; i < HISTORY_LIMIT + 5; i++) h = pushHistory(h, { method: `m${i}`, params: '' });
    expect(h).toHaveLength(HISTORY_LIMIT);
    expect(h[0].method).toBe(`m${HISTORY_LIMIT + 4}`);
  });
});
