<script lang="ts">
  /**
   * AssistantTray — a strip on top of the composer, like a tab on its edge, with what the conversation has going
   * on: its plan, its sub-agents, the edits waiting for review. The plan's segment takes the room and names the
   * step being worked on ("Plan 3/6 · Running the tests"), with the plan's progress as a line along the strip's
   * foot; the others say at a glance whether there is anything ("2 running", "3 to review"). Pressing a segment
   * opens its panel over the thread, where the full list is. Only one is open at a time; Esc or a press outside
   * closes it, pressing the segment again too. The palette opens the same panels.
   *
   * The cards stay in the thread as the record of what happened; this is where to look at what is happening now,
   * without scrolling back to find the card.
   */
  import { onMount } from 'svelte';
  import { Bot, FilePenLine } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StackedBar from '$lib/components/shared/ui/StackedBar.svelte';
  import TrayButton from '$lib/components/shared/ui/TrayButton.svelte';
  import TrayPanel from '$lib/components/shared/ui/TrayPanel.svelte';
  import DiffStat from '$lib/components/shared/ui/DiffStat.svelte';
  import { bennuAgentsStore } from '$lib/stores/bennu/assistant-agents.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { bennuAssistantTray } from '$lib/stores/bennu/assistant-tray.svelte';
  import AssistantAgentsList from './AssistantAgentsList.svelte';
  import AssistantPlanIcon from './AssistantPlanIcon.svelte';
  import AssistantPlanSteps from './AssistantPlanSteps.svelte';
  import { formatCost } from './assistant-format';
  import { latestPlan, planHeadline, planProgress, planSegments } from './assistant-plan';
  import { pendingProposals } from './assistant-proposal';

  const store = bennuAssistantStore;
  const agents = bennuAgentsStore;
  const tray = bennuAssistantTray;

  let bar = $state<HTMLElement | null>(null);

  // The agents store listens for its events once, and follows the open conversation.
  onMount(() => { void agents.start(); });
  $effect(() => { void agents.refresh(store.activeId); });
  // Another conversation: what was open belonged to the one before.
  $effect(() => { void store.activeId; tray.close(); });

  const messages = $derived(store.active?.messages ?? []);
  const plan = $derived(latestPlan(messages));
  const progress = $derived(plan ? planProgress(plan.steps) : null);
  const planLive = $derived(!!progress && progress.total > 0 && !progress.finished);
  // Plain numbers for the icon: a snippet does not see the `{#if}` that narrowed `progress`.
  const planDone = $derived(progress?.done ?? 0);
  const planTotal = $derived(progress?.total ?? 0);

  const agentList = $derived(agents.list(store.activeId));
  const running = $derived(agentList.filter((a) => a.status === 'running').length);
  const agentsCost = $derived(agentList.reduce((sum, a) => sum + a.cost, 0));

  const edits = $derived(pendingProposals(messages));

  const nothing = $derived(!plan && agentList.length === 0 && edits.length === 0);

  // The step the strip names: the one in progress, or the next one still to do. Its text without Markdown marks.
  const nowStep = $derived.by(() => {
    if (!plan || !progress || progress.finished) return '';
    const at = progress.current ?? plan.steps.findIndex((st) => st.status !== 'completed');
    return at < 0 ? '' : plan.steps[at].text.replace(/[`*_]/g, '');
  });

  function review(id: string) {
    if (!store.activeId) return;
    tray.close();
    store.openReview(store.activeId, id);
  }

  const fileName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
</script>

{#if store.active && !nothing}
  <div class="tray">
    {#if tray.open === 'plan' && plan && progress}
      <TrayPanel label="Plan" keep={bar} onclose={() => tray.close()}>
        <div class="phead" class:finished={progress.finished}>
          <AssistantPlanIcon done={progress.done} total={progress.total} active={planLive} />
          <span class="ptitle">Plan</span>
          <span class="where">{planHeadline(progress)}</span>
          <span class="count">{progress.percent}%</span>
        </div>
        <StackedBar segments={planSegments(progress)} max={progress.total} height={4} ariaLabel="Plan progress" />
        <AssistantPlanSteps steps={plan.steps} center />
      </TrayPanel>
    {:else if tray.open === 'agents' && agentList.length}
      <TrayPanel label="Agents" keep={bar} onclose={() => tray.close()}>
        <div class="phead">
          <Bot size={13} />
          <span class="ptitle">Agents</span>
          <span class="where">{running ? `${running} running` : `${agentList.length} finished`} · {formatCost(agentsCost)}</span>
        </div>
        <AssistantAgentsList />
      </TrayPanel>
    {:else if tray.open === 'edits' && edits.length}
      <TrayPanel label="Edits to review" keep={bar} onclose={() => tray.close()}>
        <div class="phead">
          <FilePenLine size={13} />
          <span class="ptitle">Edits to review</span>
          <span class="where">{edits.length === 1 ? 'one proposal' : `${edits.length} proposals`}</span>
        </div>
        <ul class="edits">
          {#each edits as e (e.id)}
            <li>
              <span class="files" title={e.files.map((f) => f.path).join('\n')}>
                {e.files.slice(0, 3).map((f) => fileName(f.path)).join(', ')}{e.files.length > 3 ? ` and ${e.files.length - 3} more` : ''}
              </span>
              <DiffStat added={e.files.reduce((n, f) => n + f.added, 0)} removed={e.files.reduce((n, f) => n + f.removed, 0)} />
              <Button variant="secondary" size="xs" onclick={() => review(e.id)}>Review…</Button>
            </li>
          {/each}
        </ul>
      </TrayPanel>
    {/if}

    <div class="strip" bind:this={bar} role="toolbar" aria-label="What this conversation has going on" tabindex="-1">
      {#if plan && progress}
        <TrayButton
          label="Plan"
          count={progress.finished ? 'done' : `${progress.done}/${progress.total}`}
          detail={nowStep}
          grow
          tone={progress.finished ? 'success' : planLive ? 'accent' : 'neutral'}
          pressed={tray.open === 'plan'}
          tooltip={planLive ? `${planHeadline(progress)} — every step` : 'The plan'}
          onclick={() => tray.toggle('plan')}
        >
          {#snippet icon()}<AssistantPlanIcon done={planDone} total={planTotal} active={planLive} />{/snippet}
        </TrayButton>
      {:else}
        <span class="fill"></span>
      {/if}
      {#if agentList.length}
        <span class="sep" aria-hidden="true"></span>
        <TrayButton
          label="Agents"
          count={running ? `${running} running` : agentList.length}
          tone={running ? 'accent' : 'neutral'}
          pressed={tray.open === 'agents'}
          tooltip="The sub-agents of this conversation"
          onclick={() => tray.toggle('agents')}
        >
          {#snippet icon()}{#if running}<Spinner size="xs" />{:else}<Bot size={13} />{/if}{/snippet}
        </TrayButton>
      {/if}
      {#if edits.length}
        <span class="sep" aria-hidden="true"></span>
        <TrayButton
          label="Review"
          count={edits.length}
          tone="warning"
          pressed={tray.open === 'edits'}
          tooltip="Edits Imber proposed that wait for your decision"
          onclick={() => tray.toggle('edits')}
        >
          {#snippet icon()}<FilePenLine size={13} />{/snippet}
        </TrayButton>
      {/if}
      {#if plan && progress}
        <span class="track" aria-hidden="true">
          <span class="done" style:width="{(progress.done / Math.max(1, progress.total)) * 100}%"></span>
          {#if planLive && progress.current !== null}
            <span class="doing" style:left="{(progress.done / Math.max(1, progress.total)) * 100}%" style:width="{100 / Math.max(1, progress.total)}%"></span>
          {/if}
        </span>
      {/if}
    </div>
  </div>
{/if}

<style>
  .tray { position: relative; padding: 0 12px; }
  /* A tab on the composer's top edge: rounded on top, open at the foot, over the composer's border. */
  .strip {
    position: relative; display: flex; align-items: stretch; min-width: 0; outline: none;
    margin-bottom: -1px; overflow: hidden;
    border: 1px solid var(--border-subtle); border-bottom: none;
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    background: var(--bg-elevated);
  }
  .fill { flex: 1; }
  .sep { width: 1px; margin: 7px 0; background: var(--border-subtle); flex-shrink: 0; }
  .track { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: color-mix(in srgb, var(--border-subtle) 60%, transparent); pointer-events: none; }
  .track .done { position: absolute; left: 0; top: 0; bottom: 0; background: var(--success); transition: width var(--transition-normal, 200ms) ease; }
  .track .doing {
    position: absolute; top: 0; bottom: 0;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    background-size: 200% 100%;
    animation: tray-doing 1.6s linear infinite;
  }
  @keyframes tray-doing { from { background-position: 100% 0; } to { background-position: -100% 0; } }
  @media (prefers-reduced-motion: reduce) { .track .doing { animation: none; background: var(--accent); opacity: 0.6; } }

  .phead { display: flex; align-items: center; gap: 7px; margin-bottom: 8px; color: var(--accent); }
  .phead.finished { color: var(--success); }
  .ptitle { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .where { flex: 1; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .count { font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; color: var(--text-muted); }

  .edits { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .edits li {
    display: flex; align-items: center; gap: 10px;
    padding: 5px 8px; border-radius: var(--radius-sm); background: var(--bg-base);
    font-size: var(--font-size-xs);
  }
  .files { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-code); color: var(--text-primary); }
</style>
