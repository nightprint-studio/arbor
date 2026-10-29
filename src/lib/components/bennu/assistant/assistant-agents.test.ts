import { describe, expect, it } from 'vitest';
import {
  elapsedLabel, nextSubagentsWord, parseSubagentsWord, runningCount, statusInfo, subagentsEffective, subagentsLabel, toolsLabel, upsertAgent,
} from './assistant-agents';
import type { AssistantAgent } from './assistant-types';

const agent = (id: string, status: AssistantAgent['status'] = 'running'): AssistantAgent => ({
  id, conversationId: 'c', runId: 'r', title: id, task: 't', tools: 'read', model: 'm', status, cost: 0, startedAt: 0, endedAt: null, summary: null, error: null,
});

describe('the agents list', () => {
  it('replaces an agent by id and appends a new one', () => {
    const list = [agent('a'), agent('b')];
    const next = upsertAgent(list, { ...agent('a'), status: 'done' });
    expect(next.map((a) => a.status)).toEqual(['done', 'running']);
    expect(upsertAgent(list, agent('c')).map((a) => a.id)).toEqual(['a', 'b', 'c']);
    expect(list[0].status).toBe('running');
  });

  it('counts the running ones', () => {
    expect(runningCount([agent('a'), agent('b', 'done'), agent('c', 'failed'), agent('d')])).toBe(2);
  });

  it('says how long an agent ran', () => {
    expect(elapsedLabel(0, 12_000, 99_000)).toBe('12s');
    expect(elapsedLabel(0, null, 185_000)).toBe('3m 05s');
    expect(elapsedLabel(0, 3_720_000, 0)).toBe('1h 02m');
    expect(elapsedLabel(5_000, null, 1_000)).toBe('0s');
  });

  it('gives every status a word and a tone', () => {
    expect(statusInfo('running').label).toBe('Running');
    expect(statusInfo('failed').tone).toBe('error');
    expect(toolsLabel('read+edit')).toBe('Reads and edits');
  });
});

describe('the sub-agents switch of a conversation', () => {
  it('follows the setting unless the conversation says otherwise', () => {
    expect(subagentsEffective(true, '')).toBe(true);
    expect(subagentsEffective(false, undefined)).toBe(false);
    expect(subagentsEffective(false, 'on')).toBe(true);
    expect(subagentsEffective(true, 'off')).toBe(false);
    expect(parseSubagentsWord(' ON ')).toBe('on');
    expect(parseSubagentsWord('maybe')).toBe('');
  });

  it('cycles through the three positions', () => {
    expect(nextSubagentsWord('')).toBe('on');
    expect(nextSubagentsWord('on')).toBe('off');
    expect(nextSubagentsWord('off')).toBe('');
  });

  it('names the position, and what following the setting means right now', () => {
    expect(subagentsLabel(true, '')).toBe('Follows settings (on)');
    expect(subagentsLabel(false, '')).toBe('Follows settings (off)');
    expect(subagentsLabel(false, 'on')).toBe('On');
  });
});
