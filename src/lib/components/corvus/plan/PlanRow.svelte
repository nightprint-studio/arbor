<script lang="ts">
  /** One plan in the Plans tab: its name, where it stands, and its phases as a strip of pills. */
  import { Check, Clock, X } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { Plan } from '$lib/types/corvus/plan';
  import { phaseState } from './plan-meta';

  interface Props {
    plan: Plan;
    onOpen: () => void;
  }

  let { plan, onOpen }: Props = $props();

  const steps = $derived([...plan.phases.flatMap(p => p.steps), ...plan.finally]);
  const repos = $derived(new Set(steps.map(s => s.repo.path)).size);
  const workspaces = $derived([...new Set(steps.map(s => s.repo.workspace).filter(Boolean))] as string[]);
  const tone = $derived(
    plan.status === 'running' ? 'accent' : plan.status === 'paused' || plan.status === 'failed' ? 'warning' : plan.status === 'done' ? 'success' : 'neutral',
  );
</script>

<button type="button" class="row" onclick={onOpen}>
  <span class="who">
    <span class="name">{plan.name}</span>
    <span class="sub">
      {steps.length} {steps.length === 1 ? 'step' : 'steps'} · {repos} {repos === 1 ? 'repo' : 'repos'}{workspaces.length ? ` · ${workspaces.join(', ')}` : ''}
      {#if plan.template}· template{/if}
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
  </span>
  <span class="status">
    <Badge variant="tone" size="sm" {tone} label={plan.status === 'paused' ? (plan.pause_reason ?? 'paused') : plan.status} />
  </span>
</button>

<style>
  .row {
    display: grid;
    grid-template-columns: 240px 1fr minmax(120px, auto);
    align-items: center;
    gap: 14px;
    width: 100%;
    padding: 10px 14px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    color: inherit;
    text-align: left;
    font: inherit;
    cursor: pointer;
  }
  .row:hover { border-color: var(--text-muted); }
  .who { display: flex; flex-direction: column; min-width: 0; }
  .name { font-weight: 600; }
  .sub { font-size: 11.5px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .strip { display: flex; align-items: center; min-width: 0; overflow: hidden; }
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
