<script lang="ts">
  /**
   * AssistantAgents — the sub-agents of the open conversation, above the thread.
   *
   * Only there while the conversation has any. The header says how many are running and what they have cost;
   * opened, each agent is a row: its state, its title (which opens its thread, read-only), how long it has run,
   * what it cost, and a Stop. "Stop all" ends every running one. Everything is reachable from the keyboard:
   * the header toggles with Enter or Space, the rows are buttons, and the palette has the same verbs.
   */
  import { onDestroy, onMount } from 'svelte';
  import { Bot, Square, Eye, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { elapsedLabel, statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AssistantAgent } from './assistant-types';

  const agents = bennuAgentsStore;
  const assistant = bennuAssistantStore;

  let open = $state(false);
  let now = $state(Date.now());
  let timer: ReturnType<typeof setInterval> | undefined;

  onMount(() => {
    void agents.start();
    timer = setInterval(() => { now = Date.now(); }, 1000);
  });
  onDestroy(() => clearInterval(timer));

  // The list follows the open conversation.
  $effect(() => { void agents.refresh(assistant.activeId); });

  const list = $derived(agents.list(assistant.activeId));
  const running = $derived(list.filter((a) => a.status === 'running').length);
  const total = $derived(list.reduce((sum, a) => sum + a.cost, 0));

  const rowKey = (a: AssistantAgent) => a.id;
</script>

{#if list.length > 0}
  <div class="agents">
    <Collapsible bind:open chevron>
      {#snippet header()}
        <span class="head">
          {#if running > 0}<Spinner size="xs" />{:else}<Bot size={12} />{/if}
          <span class="title">Agents</span>
          <span class="meta">
            {running > 0 ? `${running} running` : `${list.length} finished`} · {formatCost(total)}
          </span>
        </span>
      {/snippet}

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
    </Collapsible>
  </div>
{/if}

<style>
  .agents { padding: 2px 12px 4px; }
  .head { display: inline-flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .title { font-weight: 600; color: var(--text-primary); }
  .meta { color: var(--text-muted); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 150px; }
  .dim { margin-left: auto; font-size: var(--font-size-xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .dim + .dim { margin-left: 0; }
  .all { display: flex; justify-content: flex-end; padding-top: 4px; }
</style>
