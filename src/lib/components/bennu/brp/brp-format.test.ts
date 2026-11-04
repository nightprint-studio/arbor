import { describe, expect, it } from 'vitest';
import { entityDocument, toJson, toRon } from './brp-format';

describe('toRon', () => {
  it('writes scalars the way RON does', () => {
    expect(toRon(true)).toBe('true');
    expect(toRon(1.5)).toBe('1.5');
    expect(toRon('a "b"')).toBe('"a \\"b\\""');
    expect(toRon(null)).toBe('None');
  });

  it('keeps a short struct and a short list on one line', () => {
    expect(toRon({ x: 1, y: 2 })).toBe('(x: 1, y: 2)');
    expect(toRon([0, 1, 2])).toBe('[0, 1, 2]');
    expect(toRon({})).toBe('()');
    expect(toRon([])).toBe('[]');
  });

  it('quotes a key that is not an identifier', () => {
    expect(toRon({ 'my-key': 1 })).toBe('("my-key": 1)');
  });

  it('breaks a long value over lines, indented by depth', () => {
    const out = toRon({ name: 'x'.repeat(40), tags: ['y'.repeat(30), 'z'.repeat(30)] });
    expect(out.startsWith('(\n  name: "')).toBe(true);
    expect(out).toContain('\n  tags: [\n    "');
    expect(out.endsWith('\n)')).toBe(true);
  });
});

describe('toJson / entityDocument', () => {
  it('pretty-prints and survives undefined', () => {
    expect(toJson({ a: [1] })).toBe('{\n  "a": [\n    1\n  ]\n}');
    expect(toJson(undefined)).toBe('null');
  });

  it('orders an entity by type path so a copy is stable', () => {
    expect(Object.keys(entityDocument({ 'b::B': 1, 'a::A': 2 }))).toEqual(['a::A', 'b::B']);
  });
});
