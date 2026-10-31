import { describe, expect, it } from 'vitest';
import { oneEntryPerProject, type SwitcherEntry } from './project-switcher';

const entry = (root: string, name: string, workspaceId = 'w1'): SwitcherEntry => ({ workspaceId, workspaceName: 'W', colorIdx: 0, root, name });

describe('the project switcher list', () => {
  it('shows a project once however its folder is spelled', () => {
    const list = oneEntryPerProject([entry('/a/proj', 'proj'), entry('/a/proj/', ''), entry('/A/Proj', '')]);
    expect(list.map((e) => e.name)).toEqual(['proj']);
  });

  it('keeps the named spelling when the nameless one comes first', () => {
    expect(oneEntryPerProject([entry('/a/proj/', ''), entry('/a/proj', 'proj')]).map((e) => e.name)).toEqual(['proj']);
  });

  it('keeps the same folder of two workspaces apart, and drops an entry with no folder', () => {
    const list = oneEntryPerProject([entry('/a/proj', 'proj', 'w1'), entry('/a/proj', 'proj', 'w2'), entry('', 'ghost')]);
    expect(list.map((e) => e.workspaceId)).toEqual(['w1', 'w2']);
  });
});
