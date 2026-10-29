import { describe, expect, it } from 'vitest';
import { redoRequest, staleSummary, withDecision, withRemade, withUndo } from './assistant-proposal';
import type { EditProposal, ProposedFile } from './assistant-types';

const file = (path: string, extra: Partial<ProposedFile> = {}): ProposedFile => ({ path, added: 1, removed: 0, hunks: [], ...extra });
const proposal = (files: ProposedFile[], status: EditProposal['status'] = 'pending'): EditProposal => ({ id: 'pr-1', files, status });

describe('a decision that came back', () => {
  it('marks only the files that changed, and keeps the proposal', () => {
    const p = withDecision(proposal([file('a.java'), file('b.java')]), { status: 'outdated', files: [], stale: ['b.java'] });
    expect(p.status).toBe('outdated');
    expect(p.files.map((f) => f.stale)).toEqual([false, true]);
  });

  it('settles on applied or rejected otherwise', () => {
    expect(withDecision(proposal([file('a')]), { status: 'applied', files: ['a'], stale: [] }).status).toBe('applied');
    expect(withDecision(proposal([file('a')]), { status: 'rejected', files: [], stale: [] }).status).toBe('rejected');
  });
});

describe('a proposal made again', () => {
  it('takes the new files and is pending again', () => {
    const old = proposal([file('a', { stale: true })], 'outdated');
    const p = withRemade(old, { status: 'pending', files: [file('a', { added: 5 })], already_there: [] });
    expect(p.status).toBe('pending');
    expect(p.files[0]).toMatchObject({ added: 5 });
    expect(p.files[0].stale).toBeUndefined();
  });

  it('is closed when every change was in the files already', () => {
    const p = withRemade(proposal([file('a', { stale: true })], 'outdated'), { status: 'rejected', files: [], already_there: ['a'] });
    expect(p.status).toBe('rejected');
    expect(p.files[0].stale).toBe(false);
  });
});

describe('asking the assistant to redo it', () => {
  it('names the stale files, or all of them when none is marked', () => {
    const p = proposal([file('a.java', { stale: true }), file('b.java', { stale: false })], 'outdated');
    expect(redoRequest(p)).toContain('`a.java`');
    expect(redoRequest(p)).not.toContain('b.java');
    expect(redoRequest(proposal([file('a.java'), file('b.java')]))).toContain('`a.java`, `b.java`');
  });

  it('counts the stale files in the note', () => {
    expect(staleSummary(proposal([file('a', { stale: true })]))).toBe('The file changed since this was proposed');
    expect(staleSummary(proposal([file('a', { stale: true }), file('b', { stale: true })]))).toBe('2 files changed since this was proposed');
  });
});

describe('a proposal after an undo', () => {
  it('is undone once every file is back', () => {
    expect(withUndo(proposal([file('a')], 'applied'), { status: 'undone', files: ['a'], stale: [] }).status).toBe('undone');
  });

  it('stays applied when a file changed since, and nothing else about it moves', () => {
    const p = proposal([file('a')], 'applied');
    expect(withUndo(p, { status: 'applied', files: [], stale: ['a'] })).toBe(p);
  });
});
