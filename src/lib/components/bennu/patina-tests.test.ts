import { describe, it, expect } from 'vitest';
import { patinaTestMarks, patinaModDirOf } from './patina-tests';

describe('patinaTestMarks', () => {
  it('trova la freccia sull attributo e il nome della fn', () => {
    const src = 'fn helper() {}\n\n#[test]\nfn names_are_short() {\n}\n\n#[test]\n// nota\n#[inline]\npub fn other() {}\n';
    expect(patinaTestMarks(src)).toEqual([{ line: 3, name: 'names_are_short' }, { line: 7, name: 'other' }]);
  });
  it('un attributo senza fn vicina non fa niente', () => {
    expect(patinaTestMarks('#[test]\nlet x = 1;\n')).toEqual([]);
  });
  it('non confonde #[tested] o altri attributi', () => {
    expect(patinaTestMarks('#[tested]\nfn a() {}\n#[command]\nfn b() {}')).toEqual([]);
  });
});

describe('patinaModDirOf', () => {
  it('risale alla cartella prima di script/', () => {
    expect(patinaModDirOf('/g/mods/utilita/pets/script/rules.ptn')).toBe('/g/mods/utilita/pets');
    expect(patinaModDirOf('C:\\g\\mods\\pets\\script\\a\\b.ptn')).toBe('C:/g/mods/utilita/pets');
    expect(patinaModDirOf('/g/other/a.ptn')).toBeNull();
  });
});
