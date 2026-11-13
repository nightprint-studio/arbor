<script lang="ts">
  /**
   * AssistantAgentsList — the sub-agents of the open conversation, as the tray's Agents panel lists them.
   *
   * Each agent is a row: its state, its title (which opens its thread, read-only), how long it has run, what it
   * cost, and a Stop. "Stop all" ends every running one. The rows are buttons, and the palette has the same verbs.
   * The tray keeps the agents store listening and following the conversation; this only draws.
   */
  import { onDestroy, onMount } from 'svelte';
  import { Square, Eye, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { elapsedLabel, statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AssistantAgent } from './assistant-types';

  const agents = bennuAgentsStore;
  const assistant = bennuAssistantStore;

  let now = $state(Date.now());
  let timer: ReturnType<typeof setInterval> | undefined;

  onMount(() => { timer = setInterval(() => { now = Date.now(); }, 1000); });
  onDestroy(() => clearInterval(timer));

  const list = $derived(agents.list(assistant.activeId));
  const running = $derived(list.filter((a) => a.status === 'running').length);

  const rowKey = (a: AssistantAgent) => a.id;
</script>

<div class="agents">
  <RowList items={list} key={rowKey}>
    {#snippet row(a)}
      {@const info = statusInfo(a.status)}
      <Badge variant="tone" size="sm" tone={a.waiting ? 'warning' : info.tone} dot={a.status === 'running'} label={a.waiting ? 'Waiting to retry' : info.label} />
      <Button variant="ghost" size="xs" tooltip={`${a.task.slice(0, 240)} — open its thread`} onclick={() => void agents.open(a.id)}>
        <Eye size={11} /> <span class="name">{a.title}</span>
      </Button>
      <span class="dim" title={toolsLabel(a.tools)}>{a.tools === 'read+edit' ? 'edits' : 'reads'}</span>
      <span class="dim">{elapsedLabel(a.startedAt, a.endedAt, now)}</span>
      <span class="dim">{formatCost(a.cost)}</span>
      {#if a.status === 'running'}
        <IconButton tooltip="Stop this agent" size={22} variant="danger" onclick={() => void agents.stop(a.id)}><Square size={11} /></IconButton>
      {:else}
        <IconButton tooltip="Delete this agent and what was kept of its thread" size={22} onclick={() => void agents.remove(a.id, assistant.activeId)}><Trash2 size={11} /></IconButton>
      {/if}
    {/snippet}
  </RowList>

  {#if running > 1}
    <div class="all">
      <Button variant="ghost" size="xs" onclick={() => void agents.stopAll(assistant.activeId)}><Square size={11} /> Stop all</Button>
    </div>
  {/if}
</div>

<style>
  .agents { display: flex; flex-direction: column; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 150px; }
  .dim { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .dim + .dim { margin-left: 0; }
  .all { display: flex; justify-content: flex-end; padding-top: 4px; }
</style>
