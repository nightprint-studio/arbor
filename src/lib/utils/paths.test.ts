import { describe, expect, it } from 'vitest';
import { rootOwning } from './paths';

describe('rootOwning', () => {
  const roots = ['C:/work/app', 'C:/work/app/modules/core', 'D:/other'];

  it('finds the root a file is in, whatever the separators and the drive letter case', () => {
    expect(rootOwning(roots, 'c:\\work\\app\\src\\A.java')).toBe('C:/work/app');
    expect(rootOwning(roots, 'D:/other/B.java')).toBe('D:/other');
  });

  it('prefers the deepest root when one is inside another', () => {
    expect(rootOwning(roots, 'C:/work/app/modules/core/X.java')).toBe('C:/work/app/modules/core');
  });

  it('is not fooled by a sibling that shares a prefix, and says null when no root holds the file', () => {
    expect(rootOwning(roots, 'C:/work/application/A.java')).toBeNull();
    expect(rootOwning(roots, 'E:/nowhere/A.java')).toBeNull();
    expect(rootOwning([], 'C:/work/app/A.java')).toBeNull();
  });
});
