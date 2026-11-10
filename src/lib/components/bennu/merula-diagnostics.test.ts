import { describe, it, expect } from 'vitest';
import { lineColAt, consoleDiagnostic } from './merula-diagnostics';

describe('lineColAt', () => {
  it('counts lines and columns from 1', () => {
    const src = 'cps(0.5)\ntracks(\n  oops';
    expect(lineColAt(src, 0)).toEqual({ line: 1, col: 1 });
    expect(lineColAt(src, src.indexOf('oops'))).toEqual({ line: 3, col: 3 });
  });
  it('clamps an offset past the end', () => {
    expect(lineColAt('ab\ncd', 99)).toEqual({ line: 2, col: 3 });
  });
});

describe('consoleDiagnostic', () => {
  it('links a located diagnostic to its line', () => {
    const d = consoleDiagnostic('C:/m/song.merula', 'a\nbad', { message: 'unexpected', severity: 'error', start: 2, end: 5 });
    expect(d.text).toBe('song.merula:2:1  unexpected');
    expect(d.pieces?.[0].link).toEqual({ kind: 'file', path: 'C:/m/song.merula', line: 2 });
  });
  it('converts UTF-8 byte offsets, not UTF-16 units', () => {
    // "é" is 2 bytes: byte 3 is the `x` after "é\n", i.e. line 2 col 1.
    const d = consoleDiagnostic('s.merula', 'é\nx', { message: 'm', severity: 'error', start: 3, end: 4 });
    expect(d.text).toBe('s.merula:2:1  m');
  });
  it('leaves a span-less diagnostic unlinked', () => {
    const d = consoleDiagnostic('s.merula', 'x', { message: 'no span', severity: 'warning', start: null, end: null });
    expect(d.pieces).toBeUndefined();
    expect(d.text).toBe('no span');
  });
});
