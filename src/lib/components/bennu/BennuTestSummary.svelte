<script lang="ts">
  /**
   * A test run's verdict and counts — the one line that answers "did it pass".
   *
   * Sits on the right of the Run console's status row, where a program's "Finished · 1.2s"
   * sits, because it is the same statement about the same kind of thing.
   */
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuTestStore, formatDuration } from '$lib/stores/bennu/tests.svelte';

  const store = bennuTestStore;
  const counts = $derived(store.counts);

  const verdict = $derived.by(() => {
    if (store.running) return { tone: 'run', text: `Running ${store.label}…` };
    if (store.cancelled) return { tone: 'warn', text: 'Stopped' };
    if (!store.hasResults) return null;
    const bad = counts.failed + counts.errored;
    return bad > 0
      ? { tone: 'fail', text: `${bad} test${bad === 1 ? '' : 's'} failed` }
      : { tone: 'ok', text: `All ${counts.passed} test${counts.passed === 1 ? '' : 's'} passed` };
  });
</script>

{#if store.widened}
  <span class="tp-widened" use:tooltip={store.widened}>widened</span>
{/if}
{#if store.hasResults}
  <span class="tp-counts">
    <span class="ct ct-ok" class:on={counts.passed > 0}>{counts.passed}</span>
    <span class="ct ct-bad" class:on={counts.failed + counts.errored > 0}>{counts.failed + counts.errored}</span>
    <span class="ct ct-skip" class:on={counts.skipped > 0}>{counts.skipped}</span>
    {#if store.elapsedMs > 0}<span class="ct-time">{formatDuration(store.elapsedMs)}</span>{/if}
  </span>
{/if}
{#if verdict}
  <span class="tp-verdict tone-{verdict.tone}">
    {#if store.running}<Spinner size={11} />{/if}
    {verdict.text}
  </span>
{:else if !store.hasResults}
  <span class="tp-verdict tone-idle">Nothing run yet</span>
{/if}

<style>
  .tp-verdict { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-xs); font-weight: 500; }
  .tp-verdict.tone-ok { color: var(--success); }
  .tp-verdict.tone-fail { color: var(--error); }
  .tp-verdict.tone-warn { color: var(--warning); }
  .tp-verdict.tone-run { color: var(--text-secondary); }
  .tp-verdict.tone-idle { color: var(--text-disabled); font-weight: 400; }
  .tp-counts { display: inline-flex; align-items: center; gap: 6px; font-family: var(--font-code); font-size: var(--font-size-2xs); }
  /* A zero count stays grey: only a count that exists earns its colour. */
  .ct { color: var(--text-disabled); }
  .ct-ok.on { color: var(--success); font-weight: 600; }
  .ct-bad.on { color: var(--error); font-weight: 600; }
  .ct-skip.on { color: var(--text-muted); font-weight: 600; }
  .ct-time { color: var(--text-muted); }
  .tp-widened {
    padding: 1px 6px; border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs); text-transform: uppercase; letter-spacing: 0.04em;
    color: var(--warning); background: var(--warning-subtle); cursor: help;
  }
</style>
