<script lang="ts">
  /**
   * One plan in the Plans tab: its name, where it stands, its phases as a strip of pills, and the
   * actions you reach for without opening it — run / stop and delete.
   */
  import { Check, Clock, Play, Square, Trash2, X } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { Plan } from '$lib/types/corvus/plan';
  import { phaseState } from './plan-meta';

  interface Props {
    plan: Plan;
    onOpen: () => void;
    onRun: () => void;
    onStop: () => void;
    onDelete: () => void;
  }

  let { plan, onOpen, onRun, onStop, onDelete }: Props = $props();

  const steps = $derived([...plan.phases.flatMap(p => p.steps), ...plan.finally]);
  const repos = $derived(new Set(steps.map(s => s.repo.path)).size);
  const workspaces = $derived([...new Set(steps.map(s => s.repo.workspace).filter(Boolean))] as string[]);
  const running = $derived(plan.status === 'running');
  const runnable = $derived(!plan.template && !running && steps.length > 0);
  const tone = $derived(
    running ? 'accent' : plan.status === 'paused' || plan.status === 'failed' ? 'warning' : plan.status === 'done' ? 'success' : 'neutral',
  );

  /** Delete / Backspace on the focused row asks to delete, like a list in a file manager. */
  function onKeydown(e: KeyboardEvent) {
    if ((e.key === 'Delete' || e.key === 'Backspace') && !running) {
      e.preventDefault();
      onDelete();
    }
  }
</script>

<div class="row" class:running>
  <button type="button" class="open" onclick={onOpen} onkeydown={onKeydown}>
    <span class="who">
      <span class="name">{plan.name}</span>
      <span class="sub">
        {steps.length} {steps.length === 1 ? 'step' : 'steps'} · {repos} {repos === 1 ? 'repo' : 'repos'}{workspaces.length ? ` · ${workspaces.join(', ')}` : ''}
      </span>
    </span>
    <span class="strip">
      {#each plan.phases as phase, i (phase.id)}
        {#if i > 0}<span class="wire" class:waiting={phase.gate.state === 'waiting'} class:open={phase.gate.state === 'open'}></span>{/if}
        {@const st = phaseState(phase)}
        <span class="pill st-{st}">
          {#if st === 'done'}<Check size={11} />{:else if st === 'running'}<Spinner size={10} color="currentColor" />{:else if st === 'waiting'}<Clock size={11} />{:else if st === 'failed'}<X size={11} />{/if}
          {phase.name}
        </span>
      {/each}
      {#if plan.finally.length}
        <span class="wire"></span>
        <span class="pill dashed">Finally</span>
      {/if}
      {#if plan.phases.length === 0 && plan.finally.length === 0}
        <span class="empty">Empty — open it and press A to add a step</span>
      {/if}
    </span>
    <span class="status">
      <Badge variant="tone" size="sm" {tone} label={plan.template ? 'template' : plan.status === 'paused' ? (plan.pause_reason ?? 'paused') : plan.status} />
    </span>
  </button>
  <span class="actions">
    {#if running}
      <IconButton size={24} tooltip="Stop" onclick={onStop}><Square size={12} /></IconButton>
    {:else if runnable}
      <IconButton size={24} tooltip={plan.status === 'paused' ? 'Resume' : 'Run'} onclick={onRun}><Play size={12} /></IconButton>
    {/if}
    {#if !running}
      <IconButton size={24} tooltip="Delete" shortcut="Del" onclick={onDelete}><Trash2 size={12} /></IconButton>
    {/if}
  </span>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
  }
  .row:hover, .row:focus-within { border-color: var(--text-muted); }
  .row.running { border-color: color-mix(in srgb, var(--accent) 45%, transparent); }
  .open {
    display: grid;
    flex: 1;
    min-width: 0;
    grid-template-columns: 240px 1fr minmax(110px, auto);
    align-items: center;
    gap: 14px;
    padding: 10px 6px 10px 14px;
    border: 0;
    border-radius: var(--radius-lg);
    background: none;
    color: inherit;
    text-align: left;
    font: inherit;
    cursor: pointer;
  }
  .actions { display: flex; gap: 2px; padding-right: 8px; min-width: 56px; justify-content: flex-end; }
  .who { display: flex; flex-direction: column; min-width: 0; }
  .name { font-weight: 600; }
  .sub { font-size: 11.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .strip { display: flex; align-items: center; min-width: 0; overflow: hidden; }
  .empty { font-size: 11.5px; color: var(--text-muted); }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px;
    border: 1px solid var(--border);
    border-radius: 99px;
    font-size: 11.5px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .pill.dashed { border-style: dashed; }
  .st-done { color: var(--success); border-color: color-mix(in srgb, var(--success) 50%, transparent); }
  .st-running { color: var(--accent); border-color: var(--accent); }
  .st-waiting { color: var(--warning); border-color: var(--warning); }
  .st-failed { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 50%, transparent); }
  .wire { width: 22px; border-top: 2px solid var(--border); flex: none; }
  .wire.open { border-color: color-mix(in srgb, var(--success) 55%, transparent); }
  .wire.waiting { border-top-style: dashed; border-color: var(--warning); }
  .status { justify-self: end; }
</style>
