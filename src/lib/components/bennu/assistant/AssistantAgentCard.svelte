<script lang="ts">
  /**
   * AssistantAgentCard — what a sub-agent left in the thread of the conversation that started it: its title,
   * where it is, what it cost, and the first part of its answer. The card is updated in place as the agent goes
   * and is saved with the message, so it outlives the agent. "Open" shows the agent's own thread while the app
   * still has it.
   */
  import { Bot, Eye } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { elapsedLabel, statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AgentCard } from './assistant-types';

  let { agent }: { agent: AgentCard } = $props();

  const info = $derived(statusInfo(agent.status));
  const elapsed = $derived(agent.endedAt !== null ? elapsedLabel(agent.startedAt, agent.endedAt, agent.endedAt) : '');
</script>

<div class="card">
  <div class="head">
    {#if agent.status === 'running'}<Spinner size="xs" />{:else}<Bot size={13} />{/if}
    <span class="title">{agent.title}</span>
    <Badge variant="tone" size="sm" tone={info.tone} label={info.label} />
    <span class="meta">{toolsLabel(agent.tools)}{elapsed ? ` · ${elapsed}` : ''} · {formatCost(agent.cost)}</span>
    <Button variant="ghost" size="xs" tooltip="Open the agent's thread (read-only)" onclick={() => void bennuAgentsStore.open(agent.id)}>
      <Eye size={11} /> Open
    </Button>
  </div>
  {#if agent.waiting}<p class="waiting">{agent.waiting}</p>{/if}
  {#if agent.error && agent.status === 'failed'}<p class="failure">{agent.error}</p>{/if}
  {#if agent.summary}<p class="summary">{agent.summary}</p>{/if}
</div>

<style>
  .card {
    display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border-subtle);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
  }
  .head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; }
  .waiting { margin: 0; font-size: var(--font-size-xs); color: var(--warning); }
  .failure { margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--danger); overflow-wrap: anywhere; }
  .summary { margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-secondary); white-space: pre-wrap; }
</style>
