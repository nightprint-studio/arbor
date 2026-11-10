import { describe, it, expect } from 'vitest';
import { merulaOutlineRows } from './merula-outline';
import type { MerulaOutlineNode } from '$lib/components/merula/editor/merula-lang';

const sym = (kind: MerulaOutlineNode['kind'], label: string, line: number, offset: number): MerulaOutlineNode =>
  ({ id: `${kind}:${label}`, kind, label, name: label, line, offset });

describe('merulaOutlineRows', () => {
  it('maps kinds onto the shared glyph vocabulary', () => {
    const rows = merulaOutlineRows([
      sym('import', 'drums', 1, 5), sym('let', 'root', 2, 20), sym('fn', 'bass(root)', 3, 40),
    ]);
    expect(rows.map((r) => r.kind)).toEqual(['namespace', 'constant', 'function']);
    expect(rows[0].detail).toBe('import');
    expect(rows[2].name).toBe('bass(root)');
  });

  it('nests sections under their track and keeps offsets', () => {
    const track: MerulaOutlineNode = {
      ...sym('track', 'lead', 5, 100),
      children: [sym('section', 'INTRO', 6, 120), sym('section', 'DROP', 9, 180)],
    };
    const [row] = merulaOutlineRows([track]);
    expect(row.kind).toBe('module');
    expect(row.children?.map((c) => [c.name, c.line, c.offset])).toEqual([['INTRO', 6, 120], ['DROP', 9, 180]]);
  });

  it('leaves childless tracks without a children array', () => {
    expect(merulaOutlineRows([sym('track', 't', 1, 0)])[0].children).toBeUndefined();
  });
});
