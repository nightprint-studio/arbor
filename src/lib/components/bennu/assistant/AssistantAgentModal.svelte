<script lang="ts">
  /**
   * AssistantAgentModal — one sub-agent's thread, read-only: the task it was given and what it did, drawn by the
   * same renderer as the conversation's own messages. Live while the agent runs (it refreshes every moment),
   * as it ended afterwards. Its questions never come here: they are put to you in the conversation's thread.
   */
  import { onDestroy } from 'svelte';
  import { Bot, Square } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import { statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AssistantMessage as Message } from './assistant-types';

  const agents = bennuAgentsStore;
  const view = $derived(agents.transcript);
  const agent = $derived(view?.agent ?? null);
  const info = $derived(agent ? statusInfo(agent.status) : null);

  // While it runs, its thread is read again every moment.
  let timer: ReturnType<typeof setInterval> | undefined;
  $effect(() => {
    clearInterval(timer);
    if (agent?.status === 'running') timer = setInterval(() => void agents.refreshOpen(), 1500);
    return () => clearInterval(timer);
  });
  onDestroy(() => clearInterval(timer));
</script>

{#if agents.openId}
  <Modal onClose={() => agents.close()} width="760px" height="620px" ariaLabel="Sub-agent thread">
    {#snippet header()}
      <ModalHeader onClose={() => agents.close()}>
        <Bot size={14} />
        <span class="title">{agent?.title ?? 'Agent'}</span>
        {#if info}<Badge variant="tone" size="sm" tone={info.tone} dot={agent?.status === 'running'} label={info.label} />{/if}
        {#if agent}<span class="meta">{toolsLabel(agent.tools)} · {agent.model} · {formatCost(agent.cost)}</span>{/if}
      </ModalHeader>
    {/snippet}

    <div class="body">
      {#if agents.transcriptError}
        <Alert variant="error" text={agents.transcriptError} compact />
      {:else if !view}
        <p class="note">Loading…</p>
      {:else}
        <Collapsible chevron>
          {#snippet header()}<span class="sub">The task it was given</span>{/snippet}
          <p class="task">{agent?.task}</p>
        </Collapsible>
        {#if agent?.error}<Alert variant="warning" text={`It ended early: ${agent.error}`} compact />{/if}
        {#if view.message}
          <AssistantMessage message={view.message as Message} conversationId={agent?.conversationId ?? ''} mode="manual" />
        {:else}
          <p class="note">This agent's thread is no longer kept (the app was restarted). The summary it returned is in the conversation.</p>
        {/if}
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="hint">Read-only. Its questions come to you in the conversation.</span>
        <div class="actions">
          {#if agent?.status === 'running'}
            <Button variant="ghost" size="sm" onclick={() => agent && void agents.stop(agent.id)}><Square size={12} /> Stop</Button>
          {/if}
          <Button variant="primary" size="sm" onclick={() => agents.close()}>Close</Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .meta { font-size: var(--font-size-xs); color: var(--text-muted); }
  .body { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px; }
  .sub { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .task { margin: 6px 0 0; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-secondary); white-space: pre-wrap; }
  .note { margin: 0; font-size: var(--font-size-sm); color: var(--text-muted); }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; gap: 8px; }
</style>
