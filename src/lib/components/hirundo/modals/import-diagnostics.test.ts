import { describe, expect, it } from 'vitest';
import { byteOffset, jsonError, lineError, offsetOfLine, positionFromMessage, toDiagnostic } from './import-diagnostics';

describe('import diagnostics', () => {
  it('counts bytes, not characters', () => {
    expect(byteOffset('aè', 2)).toBe(3);
    expect(byteOffset('abc', 10)).toBe(3);
  });

  it('finds a line and column', () => {
    expect(offsetOfLine('ab\ncd\nef', 2, 2)).toBe(4);
    expect(offsetOfLine('ab', 5)).toBe(2);
  });

  it('reads a position out of an engine message', () => {
    expect(positionFromMessage('0123456789', 'Unexpected token } in JSON at position 5')).toBe(5);
    expect(positionFromMessage('ab\ncd', 'error on line 2 at column 2: bad')).toBe(4);
    expect(positionFromMessage('ab', 'nothing to see')).toBeNull();
  });

  it('marks a whole line', () => {
    expect(lineError('one\ntwo\nthree', 'x', 2)).toEqual({ message: 'x', from: 4, to: 7 });
  });

  it('says nothing about valid JSON and something about broken JSON', () => {
    expect(jsonError('{"a":1}')).toBeNull();
    const e = jsonError('{"a":}')!;
    expect(e.message.length).toBeGreaterThan(0);
    expect(e.from).toBeGreaterThanOrEqual(0);
    expect(e.to).toBeGreaterThanOrEqual(e.from);
  });

  it('builds a diagnostic in bytes', () => {
    expect(toDiagnostic('èa', { message: 'm', from: 1, to: 2 })).toMatchObject({ from: 2, to: 3, severity: 'error' });
  });
});
