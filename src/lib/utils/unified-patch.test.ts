import { describe, expect, it } from 'vitest';
import { parseUnifiedPatch } from './unified-patch';

const PATCH = [
  'diff --git a/f.txt b/f.txt',
  'index 1..2 100644',
  '--- a/f.txt',
  '+++ b/f.txt',
  '@@ -1,3 +1,3 @@',
  ' keep',
  '--- a dashed line',
  '+replaced',
  ' tail',
  '\\ No newline at end of file',
  '@@ -10 +10,2 @@',
  '-old',
  '+new',
  '+extra',
].join('\n');

describe('parseUnifiedPatch', () => {
  it('skips file headers and reads both hunks', () => {
    const hunks = parseUnifiedPatch(PATCH);
    expect(hunks).toHaveLength(2);
    expect(hunks[0].old_start).toBe(1);
    expect(hunks[1].new_start).toBe(10);
  });

  it('numbers lines and keeps a deleted line that starts with dashes', () => {
    const [first] = parseUnifiedPatch(PATCH);
    expect(first.lines.map((l) => l.kind)).toEqual(['context', 'del', 'add', 'context']);
    expect(first.lines[1]).toMatchObject({ old: 2, text: '-- a dashed line' });
    expect(first.lines[2]).toMatchObject({ new: 2, text: 'replaced' });
    expect(first.lines[3]).toMatchObject({ old: 3, new: 3 });
  });

  it('handles an omitted count as one line', () => {
    const second = parseUnifiedPatch(PATCH)[1];
    expect(second.lines.map((l) => l.kind)).toEqual(['del', 'add', 'add']);
  });

  it('returns nothing for text with no hunks', () => {
    expect(parseUnifiedPatch('')).toEqual([]);
    expect(parseUnifiedPatch('Binary files differ')).toEqual([]);
  });
});
