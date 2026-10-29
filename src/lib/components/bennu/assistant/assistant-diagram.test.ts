import { describe, expect, it } from 'vitest';
import { pendingDiagrams, verdictAnswer } from './assistant-diagram';
import type { AssistantMessage } from './assistant-types';

const message = (blocks: AssistantMessage['blocks']): AssistantMessage => ({ id: 'm', role: 'assistant', at: 0, blocks });

describe('pendingDiagrams', () => {
  it('picks the diagrams waiting for a verdict', () => {
    const m = message([
      { type: 'text', text: 'here' },
      { type: 'diagram', diagram: { id: 'd1', source: 'graph TD; A-->B', status: 'checking' }, promptId: 'p1' },
    ]);
    expect(pendingDiagrams(m, new Set())).toEqual([{ promptId: 'p1', source: 'graph TD; A-->B' }]);
  });

  it('skips the ones already decided, already taken, or without a prompt', () => {
    const m = message([
      { type: 'diagram', diagram: { id: 'd1', source: 'a', status: 'ok' }, promptId: 'p1' },
      { type: 'diagram', diagram: { id: 'd2', source: 'b', status: 'checking' }, promptId: 'p2' },
      { type: 'diagram', diagram: { id: 'd3', source: 'c', status: 'checking' } },
    ]);
    expect(pendingDiagrams(m, new Set(['p2']))).toEqual([]);
  });
});

describe('verdictAnswer', () => {
  it('passes success as ok and failure as the message', () => {
    expect(verdictAnswer({ ok: true })).toEqual({ ok: true });
    expect(verdictAnswer({ ok: false, error: 'Parse error on line 2' })).toEqual({ error: 'Parse error on line 2' });
  });
});
