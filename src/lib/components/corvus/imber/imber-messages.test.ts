import { describe, expect, it } from 'vitest';
import type { ImberApproval, ImberBlock, ImberMessage, ImberToolCall } from '$lib/types/corvus/imber';
import { activityText, foldTools, pendingApproval, plainText, scheduledText, summarizeTools, upsertMessage, userText } from './imber-messages';

const msg = (id: string, blocks: ImberBlock[] = []): ImberMessage => ({ id, role: 'assistant', at: 0, blocks });
const call = (kind: ImberToolCall['kind'], ok = true): ImberToolCall => ({ id: Math.random().toString(), kind, label: 'x', detail: '', ok });
const approval = (answer: ImberApproval['answer']): ImberApproval => ({ id: 'a', tool: 'git_commit', title: 'Commit', commands: ['git commit'], effect: '', risk: 'write', answer, result: null });

describe('upsertMessage', () => {
  it('appends a new message and replaces one it already has', () => {
    const one = upsertMessage([], msg('a1'));
    expect(one).toHaveLength(1);
    const two = upsertMessage(one, msg('a1', [{ type: 'text', text: 'hi' }]));
    expect(two).toHaveLength(1);
    expect(two[0].blocks).toHaveLength(1);
    expect(upsertMessage(two, msg('a2'))).toHaveLength(2);
  });

  it('does not mutate the list it was given', () => {
    const list = [msg('a1')];
    upsertMessage(list, msg('a1', [{ type: 'text', text: 'x' }]));
    expect(list[0].blocks).toHaveLength(0);
  });
});

describe('pendingApproval', () => {
  it('finds the undecided approval that has a prompt', () => {
    const list = [msg('a1', [{ type: 'approval', approval: approval(null), promptId: 'p1' }])];
    expect(pendingApproval(list)?.promptId).toBe('p1');
  });

  it('ignores decided ones and ones with no prompt to answer', () => {
    expect(pendingApproval([msg('a1', [{ type: 'approval', approval: approval('approve'), promptId: 'p1' }])])).toBeNull();
    expect(pendingApproval([msg('a1', [{ type: 'approval', approval: approval(null) }])])).toBeNull();
    expect(pendingApproval([])).toBeNull();
  });

  it('takes the latest when there are several', () => {
    const list = [
      msg('a1', [{ type: 'approval', approval: approval(null), promptId: 'old' }]),
      msg('a2', [{ type: 'approval', approval: approval(null), promptId: 'new' }]),
    ];
    expect(pendingApproval(list)?.promptId).toBe('new');
  });
});

describe('foldTools and summarizeTools', () => {
  it('gathers consecutive receipts and leaves other blocks in place', () => {
    const blocks: ImberBlock[] = [
      { type: 'tool', call: call('read') }, { type: 'tool', call: call('read') },
      { type: 'text', text: 'ok' },
      { type: 'tool', call: call('edit') },
    ];
    const shown = foldTools(blocks);
    expect(shown.map((s) => s.type)).toEqual(['tools', 'text', 'tools']);
    expect(shown[0].type === 'tools' && shown[0].calls).toHaveLength(2);
  });

  it('says what was done in one breath, failures last', () => {
    expect(summarizeTools([call('read'), call('read'), call('read'), call('edit'), call('edit', false)])).toBe('Looked 3 times · Changed once · 1 failed');
    expect(summarizeTools([])).toBe('');
  });
});

describe('scheduledText', () => {
  it('names who woke the conversation and drops the model-facing tag', () => {
    expect(scheduledText('Morning check', '[Scheduled: Morning check] Any new commits on main?')).toBe('⏰ Morning check\n\nAny new commits on main?');
    expect(scheduledText('x', 'plain prompt')).toBe('⏰ x\n\nplain prompt');
  });
});

describe('plainText', () => {
  it('joins the text blocks only', () => {
    expect(plainText(msg('a', [{ type: 'text', text: 'one' }, { type: 'tool', call: call('read') }, { type: 'text', text: 'two' }]))).toBe('one\n\ntwo');
  });
});

describe('userText', () => {
  it('joins the text blocks of a user message', () => {
    expect(userText(msg('u', [{ type: 'text', text: 'a' }, { type: 'text', text: 'b' }]))).toBe('a\n\nb');
  });
});

describe('activityText', () => {
  const base = { running: true, retry: null, waiting: false, tool: 'thinking' };
  it('says nothing when no run is open', () => {
    expect(activityText({ ...base, running: false })).toBeNull();
  });
  it('names the tool in plain words, or thinks', () => {
    expect(activityText({ ...base, tool: 'git_force_push' })?.label).toBe('Running force push');
    expect(activityText(base)?.label).toBe('Thinking');
  });
  it('a retry replaces the label, and an approval waits for the user', () => {
    expect(activityText({ ...base, retry: { attempt: 2, max: 5, waitSecs: 8, detail: 'overloaded' } })).toEqual({ label: '', retry: 'Retrying (2/5) in 8s', detail: 'overloaded' });
    expect(activityText({ ...base, waiting: true })?.label).toBe('Waiting for you');
  });
});
