<script lang="ts">
  /**
   * One step of a plan: what it does, on which repo, and how it is going. Clicking selects it;
   * a failed step offers retry / skip right on the card, since that is the moment they matter.
   */
  import { Check, RotateCcw, SkipForward, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { PlanStep } from '$lib/types/corvus/plan';
  import { STATUS_LABEL, STEP_META, stepDetail } from './plan-meta';

  interface Props {
    step: PlanStep;
    selected?: boolean;
    /** Editing is off while the plan runs. */
    interactive?: boolean;
    onSelect: () => void;
    onRetry?: () => void;
    onSkip?: () => void;
  }

  let { step, selected = false, interactive = true, onSelect, onRetry, onSkip }: Props = $props();

  const meta = $derived(STEP_META[step.kind]);
  const Icon = $derived(meta.icon);
</script>

<div class="card st-{step.status}" class:selected data-step-id={step.id}>
  <button type="button" class="main" onclick={onSelect} aria-pressed={selected}>
    <span class="tile" style="color: {meta.color}"><Icon size={14} /></span>
    <span class="body">
      <span class="top">
        <span class="kind">{meta.label}</span>
        <span class="status">
          {#if step.status === 'running'}
            <Spinner size={11} color="currentColor" />
          {:else if step.status === 'done'}
            <Check size={12} />
          {:else if step.status === 'failed'}
            <X size={12} />
          {/if}
          {STATUS_LABEL[step.status]}
        </span>
      </span>
      <span class="repo">{step.repo.name}</span>
      <span class="detail">{stepDetail(step)}</span>
      {#if step.status === 'failed' && step.error}
        <span class="error">{step.error}</span>
      {/if}
    </span>
  </button>
  {#if step.status === 'failed' && interactive}
    <div class="actions">
      <Button size="xs" variant="secondary" onclick={onRetry}><RotateCcw size={11} /> Retry</Button>
      <Button size="xs" variant="ghost" onclick={onSkip}><SkipForward size={11} /> Skip</Button>
    </div>
  {/if}
</div>

<style>
  .card {
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .card:hover { border-color: var(--text-muted); }
  .card.selected { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
  .main {
    display: flex;
    gap: 10px;
    width: 100%;
    padding: 10px 12px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
  }
  .body { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .top { display: flex; align-items: center; gap: 8px; }
  .kind { font-weight: 600; font-size: var(--font-size-sm); }
  .status { margin-left: auto; display: inline-flex; align-items: center; gap: 4px; font-size: 11px; color: var(--text-muted); }
  .repo { font-size: var(--font-size-sm); color: var(--text-primary); }
  .detail {
    font-family: var(--font-ui-mono, monospace);
    font-size: 11.5px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error { font-family: var(--font-ui-mono, monospace); font-size: 11.5px; color: var(--danger); white-space: pre-wrap; }
  .actions { display: flex; gap: 6px; padding: 0 12px 10px 48px; }

  .st-running { border-color: color-mix(in srgb, var(--accent) 55%, transparent); }
  .st-running .status { color: var(--accent); }
  .st-done .status { color: var(--success); }
  .st-done .kind { color: var(--text-muted); }
  .st-failed { border-color: color-mix(in srgb, var(--danger) 60%, transparent); background: color-mix(in srgb, var(--danger) 8%, var(--bg-elevated)); }
  .st-failed .status { color: var(--danger); }
  .st-held { opacity: 0.6; }
  .st-skipped { opacity: 0.45; border-style: dashed; }
</style>
