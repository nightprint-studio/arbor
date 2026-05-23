<!--
  ConflictActionBar — file-level action toolbar above the diff columns.

  Layout:
    [◀ prev]  [N / M]  [next ▶]      ────────────────       [✓ Confirm / Stage]

  Left side: prev/next navigator (only when there's at least one conflict
  region). Right side: a single primary action that either confirms the
  file's resolution (stash blocking mode, merge mode with diff regions) or
  shows a "File staged" / "Resolution: …" status pill when the file is done.

  Two reasons it lives in its own component:
    1. Both `merge` and `stash` modes render the same bar with different
       labels — keeping the shape (props) consistent makes the consumer
       template much shorter.
    2. The "no diff regions but still confirmable" empty-state in stash mode
       hides only the nav buttons but keeps the right-side action — pushing
       that conditional inside the bar keeps the parent simpler.
-->
<script lang="ts">
  import { ChevronUp, ChevronDown, PackageCheck, CheckCircle2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Number of conflict regions in the current file; 0 hides the nav cluster. */
    regionCount:   number;
    /** 1-based index of the active region (or -1 if none). */
    activeIndex:   number;
    onPrev:        () => void;
    onNext:        () => void;

    // Right-side action:
    //   action: 'idle'   → render the confirm button (calls `onAction`)
    //   action: 'busy'   → render the confirm button disabled with loading text
    //   action: 'done'   → render a static "done" pill instead of a button
    action:        'idle' | 'busy' | 'done';
    actionLabel:   string;
    /** Tooltip surfaced on the confirm button. */
    actionTooltip?: string;
    doneLabel:     string;
    onAction:      () => void;

    /** Hint for prev/next tooltip ("conflict" vs "diff"). */
    navTooltipNoun?: string;
  }

  let {
    regionCount, activeIndex, onPrev, onNext,
    action, actionLabel, actionTooltip = '', doneLabel, onAction,
    navTooltipNoun = 'conflict',
  }: Props = $props();
</script>

<div class="conflict-nav">
  {#if regionCount > 0}
    <button class="cnav-btn" onclick={onPrev} use:tooltip={`Previous ${navTooltipNoun}`}>
      <ChevronUp size={12} />
    </button>
    <span class="cnav-counter">
      {activeIndex >= 0 ? activeIndex + 1 : '—'} / {regionCount}
    </span>
    <button class="cnav-btn" onclick={onNext} use:tooltip={`Next ${navTooltipNoun}`}>
      <ChevronDown size={12} />
    </button>
  {/if}
  <div class="cnav-spacer"></div>
  {#if action === 'done'}
    <span class="cnav-staged">
      <CheckCircle2 size={12} />
      {doneLabel}
    </span>
  {:else}
    <button
      class="cnav-stage"
      onclick={onAction}
      disabled={action === 'busy'}
      use:tooltip={actionTooltip}
    >
      <PackageCheck size={12} />
      {actionLabel}
    </button>
  {/if}
</div>

<style>
  .conflict-nav {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
    flex-shrink: 0;
    font-family: var(--font-ui-sans);
  }

  .cnav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .cnav-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .cnav-counter {
    font-size: 11px;
    color: var(--text-muted);
    min-width: 40px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .cnav-spacer { flex: 1; }

  .cnav-stage {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent);
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), opacity var(--transition-fast);
  }
  .cnav-stage:hover:not(:disabled) { background: var(--accent-hover); }
  .cnav-stage:disabled { opacity: 0.5; cursor: not-allowed; }

  .cnav-staged {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 500;
    color: var(--success);
    font-family: var(--font-ui-sans);
  }
</style>
