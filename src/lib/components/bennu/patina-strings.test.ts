import { describe, expect, it } from 'vitest';
import { stringContext } from './patina-strings';

describe('where the caret is in a Patina string', () => {
  it('is outside a string, in its text, or in a hole', () => {
    expect(stringContext('let a = ')).toBe('none');
    expect(stringContext('let a = t!("ui:av')).toBe('text');
    expect(stringContext('let a = "hi {pl')).toBe('hole');
    expect(stringContext('let a = "hi {p.')).toBe('hole');
  });

  it('closes a hole and a string', () => {
    expect(stringContext('let a = "hi {p} and ')).toBe('text');
    expect(stringContext('let a = "x" + ')).toBe('none');
  });

  it('reads {{ }} as literal braces and \\ as an escape', () => {
    expect(stringContext('let a = "{{pl')).toBe('text');
    expect(stringContext('let a = "a}}b {x')).toBe('hole');
    expect(stringContext('let a = "q\\"{')).toBe('hole');
    expect(stringContext('let a = "q\\"')).toBe('text');
  });

  it('ignores a quote in a comment and a hole in a raw string', () => {
    expect(stringContext('let a = 1 // "x {y')).toBe('none');
    expect(stringContext('let a = r"x {y')).toBe('text');
    expect(stringContext('let br = "x {y')).toBe('hole');
  });
});
