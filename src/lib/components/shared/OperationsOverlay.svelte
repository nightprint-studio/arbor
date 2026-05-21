<script lang="ts">
  import { fly, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { X, CheckCircle2, AlertTriangle, ChevronDown, ChevronRight } from 'lucide-svelte';
  import ProgressStepper from '$lib/components/shared/ui/ProgressStepper.svelte';
  import { operationsStore } from '$lib/stores/operations.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Live ticker for "started 3s ago" labels ───────────────────────────────
  let tick = $state(0);
  $effect(() => {
    if (operationsStore.operations.length === 0) return;
    const id = setInterval(() => { tick++; }, 1000);
    return () => clearInterval(id);
  });

  function ago(startedAt: number): string {
    void tick;
    const secs = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
    if (secs < 60)  return `${secs}s`;
    const m = Math.floor(secs / 60);
    return `${m}m ${secs % 60}s`;
  }

  // Per-card collapsed state (default collapsed). Lives outside the store so
  // the user's expand/collapse choice is purely UI and doesn't pollute the
  // operation model. Cleared lazily — stale entries for dismissed ops are
  // harmless (Maps are tiny and rebuilt on reload).
  let collapsed = $state(new Map<string, boolean>());

  function isCollapsed(id: string): boolean {
    const v = collapsed.get(id);
    return v === undefined ? true : v;
  }

  function toggleCollapsed(id: string) {
    const next = new Map(collapsed);
    next.set(id, !isCollapsed(id));
    collapsed = next;
  }
</script>

{#if operationsStore.operations.length > 0}
  <div class="ops-overlay" role="region" aria-label="Active operations">
    {#each operationsStore.operations as op (op.id)}
      {@const isOpen = !isCollapsed(op.id)}
      {@const totalSteps = op.steps.length}
      {@const currentIdx = op.current ? op.steps.findIndex(s => s.key === op.current) : -1}
      {@const currentStep = currentIdx >= 0 ? op.steps[currentIdx] : null}
      <div
        class="ops-card"
        class:is-done={op.done}
        class:has-error={!!op.error}
        class:collapsed={!isOpen}
        in:fly|global={{ x: 360, duration: animStore.dPanel, easing: cubicOut }}
        out:fly|global={{ x: 360, duration: animStore.dPanel, easing: cubicOut, opacity: 0 }}
      >
        <!-- Whole header is the toggle target — bigger hit-area than a tiny
             chevron. The dismiss control inside is a real button and stops
             propagation so it doesn't accidentally also toggle. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="ops-header"
          role="button"
          tabindex="0"
          aria-expanded={isOpen}
          onclick={() => toggleCollapsed(op.id)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleCollapsed(op.id); } }}
        >
          <span class="ops-chevron" aria-hidden="true">
            {#if isOpen}
              <ChevronDown size={13} />
            {:else}
              <ChevronRight size={13} />
            {/if}
          </span>
          <div class="ops-title-wrap">
            <span class="ops-title">
              {#if op.done && op.error}
                <AlertTriangle size={13} class="ops-icon ops-icon-err" />
              {:else if op.done}
                <CheckCircle2 size={13} class="ops-icon ops-icon-ok" />
              {/if}
              {op.title}
            </span>
            {#if !isOpen && !op.done && currentStep}
              <span class="ops-subtitle ops-current-line">
                <span class="ops-step-counter">{Math.max(currentIdx + 1, 1)}/{totalSteps}</span>
                {currentStep.label}{op.activeDetail ? ` — ${op.activeDetail}` : ''}
              </span>
            {:else if op.subtitle}
              <span class="ops-subtitle">{op.subtitle}</span>
            {/if}
          </div>
          <div class="ops-meta">
            {#if !op.done}
              <span class="ops-elapsed">{ago(op.startedAt)}</span>
            {/if}
            <button
              class="ops-close"
              type="button"
              aria-label="Dismiss"
              use:tooltip={'Dismiss'}
              onclick={(e) => { e.stopPropagation(); operationsStore.dismiss(op.id); }}
            >
              <X size={12} />
            </button>
          </div>
        </div>

        {#if !op.done && !isOpen}
          <div class="ops-progress-bar" aria-hidden="true">
            <span
              class="ops-progress-fill"
              style="width: {totalSteps > 0 ? Math.max(0, Math.min(100, ((currentIdx + 1) / totalSteps) * 100)) : 0}%"
            ></span>
          </div>
        {/if}

        {#if isOpen}
          <div class="ops-body" transition:slide={{ duration: animStore.dBase }}>
            <ProgressStepper
              steps={op.steps}
              current={op.current}
              activeDetail={op.activeDetail}
              done={op.done && !op.error}
              error={op.done ? op.error : null}
              layout="vertical"
              size="sm"
            />
          </div>
        {/if}

        {#if op.done && op.summary && isOpen}
          <div class="ops-summary" class:err={!!op.error} transition:slide={{ duration: animStore.dBase }}>{op.summary}</div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  /* Stack of operation cards.  Renders inline (no fixed positioning) — the
     parent (`.bottom-right-stack` in AppShell) handles positioning and
     z-index so the cards stack with toasts and notifications.  Operations
     always sit at the bottom of that stack (anchored above the status bar)
     because the OperationsOverlay is rendered as the LAST child. */
  .ops-overlay {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 340px;
    max-width: calc(100vw - 24px);
  }

  .ops-card {
    pointer-events: auto;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
    overflow: hidden;
    font-family: var(--font-ui-sans);
    /* Cap height — the stepper inside scrolls when there are many steps
       (workspace fetch-all on a 30-repo workspace, etc). */
    max-height: 60vh;
    display: flex;
    flex-direction: column;
  }

  .ops-card.is-done { opacity: 0.92; }
  .ops-card.has-error { border-color: color-mix(in srgb, var(--danger, #c94a4a) 60%, var(--border)); }

  /* ── Header ────────────────────────────────────────────────────────── */
  /* The header is a button so the whole strip toggles collapse — much
     bigger hit-area than a tiny chevron. */
  .ops-header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px 6px;
    border-bottom: 1px solid var(--border-subtle);
    width: 100%;
    background: transparent;
    border-left: none;
    border-right: none;
    border-top: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .ops-header:hover { background: var(--bg-hover); }
  .ops-card.collapsed .ops-header { border-bottom: none; }

  .ops-chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 14px;
    height: 18px;
    color: var(--text-muted);
    margin-top: 1px;
  }
  .ops-current-line {
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
  }
  .ops-step-counter {
    color: var(--accent);
    font-weight: 600;
    margin-right: 6px;
  }

  /* Slim progress bar shown only when collapsed + still running. */
  .ops-progress-bar {
    height: 2px;
    background: var(--bg-overlay);
    overflow: hidden;
  }
  .ops-progress-fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width var(--transition-base);
  }
  .ops-title-wrap { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .ops-title {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ops-subtitle {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ops-meta { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .ops-elapsed {
    font-size: 10px;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
    padding: 1px 5px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-radius: var(--radius-sm);
  }
  .ops-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ops-close:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Body (stepper) ────────────────────────────────────────────────── */
  .ops-body {
    padding: 8px 10px;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }

  /* ── Summary ───────────────────────────────────────────────────────── */
  .ops-summary {
    padding: 6px 10px 8px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .ops-summary.err { color: var(--danger, #c94a4a); }

  :global(.ops-icon-ok)  { color: var(--success); }
  :global(.ops-icon-err) { color: var(--danger, #c94a4a); }
</style>
