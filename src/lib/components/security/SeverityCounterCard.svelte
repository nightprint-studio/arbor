<script lang="ts">
  /**
   * Single counter card in the security dashboard grid.
   * Mirrors the GitLab Security Dashboard tile: large severity-coloured
   * count + median-age subtitle. Click handler is owned by the parent so
   * the grid can route every card to the same detail modal (Phase 4).
   */
  import { SEVERITY_META, formatMedianAge } from './severity-meta';
  import type { Severity } from '$lib/types/security';

  interface Props {
    severity: Severity;
    count:    number;
    /** Median age in days for findings of this severity (null if zero). */
    medianAgeDays: number | null;
    onclick?: () => void;
  }

  let { severity, count, medianAgeDays, onclick }: Props = $props();

  const meta     = $derived(SEVERITY_META[severity]);
  const ageLabel = $derived(formatMedianAge(medianAgeDays));
  const empty    = $derived(count === 0);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<button
  class="sev-card"
  class:empty
  style:--sev-color={meta.color}
  style:--sev-bg={meta.bgColor}
  type="button"
  onclick={() => onclick?.()}
  aria-label="{meta.label}: {count} findings"
>
  <span class="sev-label">{meta.label}</span>
  <span class="sev-count">{count}</span>
  <span class="sev-age">{empty ? '—' : ageLabel}</span>
</button>

<style>
  .sev-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 12px 14px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-left: 3px solid var(--sev-color);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast), border-color var(--transition-fast), transform var(--transition-fast);
  }
  .sev-card:hover:not(.empty) {
    background: var(--sev-bg);
    border-color: var(--sev-color);
    transform: translateY(-1px);
  }
  .sev-card.empty { opacity: 0.6; cursor: default; }
  .sev-card.empty:hover { transform: none; }

  .sev-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--sev-color);
  }
  .sev-count {
    font-size: 22px;
    font-weight: 700;
    line-height: 1;
    color: var(--text-primary);
  }
  .sev-age {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: 2px;
  }
</style>
