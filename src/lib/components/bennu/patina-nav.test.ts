import { describe, it, expect } from 'vitest';
import { hasDeclarationEngine, opensReadOnly } from './patina-nav';

describe('hasDeclarationEngine', () => {
  it('asks the declaration engine for a Patina script, which the old check silently skipped', () => {
    expect(hasDeclarationEngine('/m/script/a.ptn', false, false)).toBe(true);
    expect(hasDeclarationEngine('/m/script/A.PTN', false, false)).toBe(true);
  });
  it('still asks for Java and for a language-server file, and for nothing else', () => {
    expect(hasDeclarationEngine('/p/A.java', true, false)).toBe(true);
    expect(hasDeclarationEngine('/p/a.rs', false, true)).toBe(true);
    expect(hasDeclarationEngine('/p/page.jsp', false, false)).toBe(false);
    expect(hasDeclarationEngine('/p/struts.xml', false, false)).toBe(false);
    expect(hasDeclarationEngine(null, false, false)).toBe(false);
  });
});

describe('opensReadOnly', () => {
  const root = '/home/c/repo/m';
  it('opens a file of the project for editing', () => {
    expect(opensReadOnly('/home/c/repo/m/script/other.ptn', root)).toBe(false);
  });
  it('opens a dependency mod beside the project read-only', () => {
    expect(opensReadOnly('/home/c/repo/marmo/script/lib.ptn', root)).toBe(true);
  });
  it('is not fooled by a sibling folder that starts with the same name', () => {
    expect(opensReadOnly('/home/c/repo/m2/script/lib.ptn', root)).toBe(true);
  });
  it('opens a library view written by the backend read-only, wherever the project is', () => {
    expect(opensReadOnly('/data/arbor/bennu/decompiled/patina/voriz/world.ptn', root)).toBe(true);
    expect(opensReadOnly('C:\\Data\\bennu\\decompiled\\patina\\voriz\\world.ptn', 'C:\\Data\\bennu\\decompiled\\patina')).toBe(true);
  });
  it('reads Windows separators and ignores case', () => {
    expect(opensReadOnly('C:\\Repo\\M\\script\\a.ptn', 'c:/repo/m')).toBe(false);
  });
  it('edits when there is no project to compare with', () => {
    expect(opensReadOnly('/anywhere/a.ptn', null)).toBe(false);
  });
});
