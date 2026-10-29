/**
 * The small, pure rules behind the Agents list: how a list changes when an agent reports, how long one has run,
 * what its status looks like, and the three-way sub-agents switch of a conversation.
 */

import type { AgentStatus, AssistantAgent } from './assistant-types';

/** Replace the agent with the same id, or append it — the backend sends the whole entry each time. */
export function upsertAgent(list: readonly AssistantAgent[], agent: AssistantAgent): AssistantAgent[] {
  const at = list.findIndex((a) => a.id === agent.id);
  return at >= 0 ? list.map((a, i) => (i === at ? agent : a)) : [...list, agent];
}

export function runningCount(list: readonly AssistantAgent[]): number {
  return list.filter((a) => a.status === 'running').length;
}

/** `12s`, `3m 05s`, `1h 02m` — how long an agent has run (or ran). */
export function elapsedLabel(startedAt: number, endedAt: number | null, now: number): string {
  const secs = Math.max(0, Math.round(((endedAt ?? now) - startedAt) / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${String(secs % 60).padStart(2, '0')}s`;
  return `${Math.floor(mins / 60)}h ${String(mins % 60).padStart(2, '0')}m`;
}

const STATUS: Record<AgentStatus, { label: string; tone: 'info' | 'success' | 'warning' | 'error' }> = {
  running: { label: 'Running', tone: 'info' },
  done: { label: 'Done', tone: 'success' },
  stopped: { label: 'Stopped', tone: 'warning' },
  failed: { label: 'Failed', tone: 'error' },
};

export function statusInfo(status: AgentStatus): { label: string; tone: 'info' | 'success' | 'warning' | 'error' } {
  return STATUS[status] ?? STATUS.failed;
}

/** What an agent may do, in the words the list uses. */
export function toolsLabel(tools: 'read' | 'read+edit'): string {
  return tools === 'read+edit' ? 'Reads and edits' : 'Reads';
}

/** A conversation's own sub-agents switch: nothing follows the setting. */
export type SubagentsWord = '' | 'on' | 'off';

export function parseSubagentsWord(word: string | undefined | null): SubagentsWord {
  const w = (word ?? '').trim().toLowerCase();
  return w === 'on' || w === 'off' ? w : '';
}

/** Whether sub-agents are offered to a conversation: its own word when it has one, else the setting. */
export function subagentsEffective(setting: boolean, word: string | undefined | null): boolean {
  const w = parseSubagentsWord(word);
  return w === '' ? setting : w === 'on';
}

/** The next position when the switch is cycled: follow the settings → on → off → follow the settings. */
export function nextSubagentsWord(word: string | undefined | null): SubagentsWord {
  const w = parseSubagentsWord(word);
  return w === '' ? 'on' : w === 'on' ? 'off' : '';
}

/** How the composer's menu names the switch. */
export function subagentsLabel(setting: boolean, word: string | undefined | null): string {
  const w = parseSubagentsWord(word);
  if (w === 'on') return 'On';
  if (w === 'off') return 'Off';
  return setting ? 'Follows settings (on)' : 'Follows settings (off)';
}
