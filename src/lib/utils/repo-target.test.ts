import { describe, expect, it } from 'vitest';
import { choiceFromValue, choiceValue, defaultChoice, normPath, planOpen, type WorkspaceLike } from './repo-target';

const ws = (id: string, repo_ids: string[], order = 0): WorkspaceLike => ({ id, name: id, repo_ids, order });
const registry = [{ id: 'r1', path: 'C:\\Dev\\Api' }, { id: 'r2', path: '/home/me/web' }];

describe('normPath', () => {
  it('folds separators, case and trailing slashes', () => {
    expect(normPath('C:\\Dev\\Api\\')).toBe('c:/dev/api');
    expect(normPath('/Home/Me/Web//')).toBe('/home/me/web');
  });
});

describe('planOpen', () => {
  it('brings forward the tab that is already open, however the path is spelled', () => {
    expect(planOpen('c:/dev/api/', [{ id: 't9', path: 'C:\\Dev\\Api' }], registry, [], null)).toEqual({ kind: 'activate', tabId: 't9' });
  });

  it('asks when the repository is not registered', () => {
    expect(planOpen('/somewhere/else', [], registry, [ws('a', ['r1'])], 'a')).toEqual({ kind: 'ask' });
  });

  it('stays in the active workspace when it contains the repository', () => {
    const plan = planOpen('C:/Dev/Api', [], registry, [ws('a', ['r1'], 0), ws('b', ['r1'], 1)], 'b');
    expect(plan).toEqual({ kind: 'open', repoId: 'r1', workspaceId: 'b', switchWorkspace: false });
  });

  it('goes to the FIRST workspace that contains it when the active one does not', () => {
    const plan = planOpen('C:/Dev/Api', [], registry, [ws('late', ['r1'], 5), ws('other', ['r2'], 0), ws('early', ['r1'], 1)], 'other');
    expect(plan).toEqual({ kind: 'open', repoId: 'r1', workspaceId: 'early', switchWorkspace: true });
  });

  it('opens a registered repository that belongs to no workspace without adding it to any', () => {
    expect(planOpen('/home/me/web', [], registry, [ws('a', ['r1'])], 'a')).toEqual({ kind: 'open', repoId: 'r2', workspaceId: null, switchWorkspace: false });
  });

  it('with no active workspace, still picks the first that contains it', () => {
    expect(planOpen('C:/Dev/Api', [], registry, [ws('a', ['r1'])], null)).toEqual({ kind: 'open', repoId: 'r1', workspaceId: 'a', switchWorkspace: true });
  });
});

describe('the unregistered-repository prompt', () => {
  it('starts on the active workspace, or on just opening it when there is none', () => {
    expect(defaultChoice('a', [ws('a', [])])).toEqual({ kind: 'add', workspaceId: 'a' });
    expect(defaultChoice(null, [ws('a', [])])).toEqual({ kind: 'only' });
    expect(defaultChoice('gone', [ws('a', [])])).toEqual({ kind: 'only' });
  });

  it('round-trips through the select value', () => {
    for (const c of [{ kind: 'only' as const }, { kind: 'add' as const, workspaceId: 'ws-7' }]) expect(choiceFromValue(choiceValue(c))).toEqual(c);
  });
});
