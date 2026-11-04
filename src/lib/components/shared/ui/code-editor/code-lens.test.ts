import { describe, it, expect } from 'vitest';

import { indentOf } from './code-lens';

describe('indentOf', () => {
  it('counts a space as one column', () => {
    expect(indentOf('    public void m() {')).toBe(4);
    expect(indentOf('public class A')).toBe(0);
  });

  it('runs a tab to the next multiple of the tab size', () => {
    expect(indentOf('\tpublic', 4)).toBe(4);
    expect(indentOf('\t\tpublic', 4)).toBe(8);
    expect(indentOf('  \tpublic', 4)).toBe(4);
    expect(indentOf('\tpublic', 8)).toBe(8);
  });

  it('stops at the first glyph', () => {
    expect(indentOf('  @Override')).toBe(2);
    expect(indentOf('   ')).toBe(3);
    expect(indentOf('')).toBe(0);
  });
});
