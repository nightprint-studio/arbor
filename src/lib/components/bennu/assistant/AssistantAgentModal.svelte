<script lang="ts">
  /**
   * AssistantAgentModal — one sub-agent's thread, read-only, drawn as the conversation it is: the assistant that
   * started it (badged `main`) hands it the task, the sub-agent answers in its own voice (badged `sub-agent`), and
   * when it ends it hands its summary back. The agent's own messages are drawn by the same renderer as the
   * conversation's. Live while it runs: the thread is read again every moment, the view follows the end of it
   * unless you scrolled up, and an animated mark says it is still working. Its questions never come here: they
   * are put to you in the conversation's thread.
   */
  import { onDestroy } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ArrowRight, CornerDownLeft, Square, Trash2, TriangleAlert, Wrench } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import AssistantMessage from './AssistantMessage.svelte';
  import AssistantUserText from './AssistantUserText.svelte';
  import AssistantWho from './AssistantWho.svelte';
  import { elapsedLabel, statusInfo, toolsLabel } from './assistant-agents';
  import { formatCost } from './assistant-format';
  import type { AssistantMessage as Message } from './assistant-types';

  const agents = bennuAgentsStore;
  const view = $derived(agents.transcript);
  const agent = $derived(view?.agent ?? null);
  const info = $derived(agent ? statusInfo(agent.status) : null);
  const running = $derived(agent?.status === 'running');

  let now = $state(Date.now());
  let thread = $state<HTMLElement>();

  // While it runs, its thread is read again every moment, and the clock in the header moves.
  let timer: ReturnType<typeof setInterval> | undefined;
  let clock: ReturnType<typeof setInterval> | undefined;
  $effect(() => {
    clearInterval(timer);
    clearInterval(clock);
    if (running) {
      timer = setInterval(() => void agents.refreshOpen(), 1500);
      clock = setInterval(() => { now = Date.now(); }, 1000);
    }
    return () => { clearInterval(timer); clearInterval(clock); };
  });
  onDestroy(() => { clearInterval(timer); clearInterval(clock); });

  const elapsed = $derived(agent ? elapsedLabel(agent.startedAt, agent.endedAt, now) : '');
  const startedAt = $derived(agent ? new Date(agent.startedAt).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }) : '');

  const mainWho = { name: 'Imber', kind: 'imber' as const, badge: { label: 'main', tone: 'accent' as const } };
  const subWho = $derived({
    name: agent?.title ?? 'Agent',
    kind: 'agent' as const,
    badge: { label: 'sub-agent', tone: 'info' as const },
    live: running,
  });

  // Follow the end of the thread while it grows — unless you scrolled up to read, which is yours to keep.
  $effect(() => {
    void view?.message;
    const scroller = thread?.closest<HTMLElement>('.modal-body');
    if (!scroller) return;
    const nearEnd = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 120;
    if (nearEnd) queueMicrotask(() => scroller.scrollTo({ top: scroller.scrollHeight, behavior: animStore.dFast ? 'smooth' : 'auto' }));
  });

  const fate = $derived(
    agent?.status === 'done'
      ? { text: 'Returned its summary to main', tone: 'success' }
      : agent?.status === 'stopped'
        ? { text: 'Stopped before it finished', tone: 'warning' }
        : agent?.status === 'failed'
          ? { text: 'Ended with an error', tone: 'error' }
          : null,
  );
</script>

{#if agents.openId}
  <Modal onClose={() => agents.close()} width="780px" height="660px" ariaLabel="Sub-agent thread">
    {#snippet header()}
      <ModalHeader onClose={() => agents.close()}>
        {#if running}<span class="asst-orb" aria-hidden="true"></span>{/if}
        <span class="title">{agent?.title ?? 'Agent'}</span>
        {#if info}<Badge variant="tone" size="sm" tone={agent?.waiting ? 'warning' : info.tone} dot={running} label={agent?.waiting ? 'Waiting to retry' : info.label} />{/if}
        {#if agent}
          <span class="chips">
            <Badge variant="chip" size="sm" label={toolsLabel(agent.tools)} />
            <Badge variant="chip" size="sm" label={agent.model} />
            <Badge variant="chip" size="sm" label={elapsed} />
            <Badge variant="chip" size="sm" label={formatCost(agent.cost)} />
          </span>
        {/if}
      </ModalHeader>
    {/snippet}

    <div class="thread" bind:this={thread}>
      {#if running}<div class="asst-sweep edge" aria-hidden="true"></div>{/if}

      {#if agents.transcriptError}
        <Alert variant="error" text={agents.transcriptError} compact />
      {:else if !view}
        <div class="loading"><span class="asst-dots" aria-hidden="true"><i></i><i></i><i></i></span> Loading the thread…</div>
      {:else}
        <!-- What main asked, as the message it was. -->
        <section class="turn" in:fly={{ y: 8, duration: animStore.dFast, easing: cubicOut }}>
          <AssistantWho name={mainWho.name} kind={mainWho.kind} badge={mainWho.badge} when={startedAt} />
          <div class="ask">
            <div class="ask-to"><ArrowRight size={11} /> to <strong>{agent?.title}</strong></div>
            <div class="ask-text"><AssistantUserText text={agent?.task ?? ''} /></div>
          </div>
        </section>

        {#if agent?.error}<Alert variant="warning" text={`It ended early: ${agent.error}`} compact />{/if}

        <!-- What the sub-agent did and answered. -->
        {#if view.message}
          <section class="turn">
            <AssistantMessage message={view.message as Message} conversationId={agent?.conversationId ?? ''} mode="manual" author={subWho} />
          </section>
        {:else}
          <p class="note">This agent's thread is no longer kept (the app was restarted). The summary it returned is in the conversation.</p>
        {/if}

        {#if running}
          <div class="working" in:fly={{ y: 6, duration: animStore.dFast, easing: cubicOut }}>
            <span class="asst-dots" aria-hidden="true"><i></i><i></i><i></i></span>
            <span class="asst-shimmer">{agent?.waiting ?? 'working…'}</span>
          </div>
        {:else if fate}
          <div class="fate tone-{fate.tone}" in:fly={{ y: 6, duration: animStore.dFast, easing: cubicOut }}>
            {#if fate.tone === 'success'}<CornerDownLeft size={12} />{:else if fate.tone === 'warning'}<Wrench size={12} />{:else}<TriangleAlert size={12} />{/if}
            <span>{fate.text}</span>
            <span class="fate-meta">{elapsed} · {formatCost(agent?.cost ?? 0)}</span>
          </div>
        {/if}
      {/if}
    </div>

    {#snippet footer()}
      <ModalFooter align="between">
        <span class="hint">Read-only. Its questions come to you in the conversation. Kept until the conversation is compacted or deleted.</span>
        <div class="actions">
          {#if running}
            <Button variant="ghost" size="sm" onclick={() => agent && void agents.stop(agent.id)}><Square size={12} /> Stop</Button>
          {:else if agent}
            <Button variant="ghost" size="sm" tooltip="Delete this agent and what was kept of its thread" onclick={() => void agents.remove(agent.id, bennuAssistantStore.activeId)}><Trash2 size={12} /> Delete</Button>
          {/if}
          <Button variant="primary" size="sm" onclick={() => agents.close()}>Close</Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chips { display: inline-flex; flex-wrap: wrap; gap: 4px; margin-left: 6px; }

  .thread { display: flex; flex-direction: column; gap: 16px; padding: 4px 4px 8px; }
  /* The sweep runs along the top of the thread while the agent works. */
  .edge { margin: -4px -4px 0; border-radius: 2px; }
  .turn { display: flex; flex-direction: column; gap: 7px; min-width: 0; }

  /* Main's message: a bubble with the same family as yours, offset so it reads as one side of a two-way chat. */
  .ask {
    margin-left: 25px; max-width: 92%;
    padding: 9px 12px;
    border-radius: 4px 14px 14px 14px;
    background: color-mix(in srgb, var(--accent) 11%, var(--bg-elevated));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 18%, transparent);
    font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-primary);
  }
  .ask-to {
    display: inline-flex; align-items: center; gap: 4px; margin-bottom: 4px;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .ask-to strong { color: var(--text-secondary); font-weight: 600; }
  .ask-text { user-select: text; }

  .working {
    display: flex; align-items: center; gap: 8px; margin-left: 25px;
    font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .fate {
    display: flex; align-items: center; gap: 7px; margin-left: 25px;
    padding: 6px 10px; border-radius: var(--radius-md);
    font-size: var(--font-size-xs);
    border: 1px dashed color-mix(in srgb, currentColor 35%, transparent);
  }
  .fate.tone-success { color: var(--success); }
  .fate.tone-warning { color: var(--warning); }
  .fate.tone-error { color: var(--danger); }
  .fate-meta { margin-left: auto; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .loading { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-sm); color: var(--text-muted); }
  .note { margin: 0; font-size: var(--font-size-sm); color: var(--text-muted); }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .actions { display: flex; align-items: center; gap: 8px; }
</style>
