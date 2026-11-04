<script lang="ts">
  /**
   * The matches of a run as a compact table: one row per match (index, text, span), its groups on
   * lines beneath, each group chip in the tint the test text paints it with. Hovering a row emphasises
   * that range in the test text.
   */
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { RegexMatch } from './types';

  let {
    matches,
    groupNames,
    truncated = false,
    ran = false,
    hasText = false,
    onhover,
  }: {
    matches: RegexMatch[];
    groupNames: (string | null)[];
    truncated?: boolean;
    /** A run has completed — tells "no matches" from "nothing to show yet". */
    ran?: boolean;
    /** There is test text to match against. */
    hasText?: boolean;
    onhover?: (range: { from: number; to: number } | null) => void;
  } = $props();

  /** Rendering thousands of rows would cost more than the information is worth. */
  const SHOWN = 300;
  const visible = $derived(matches.slice(0, SHOWN));

  /** A visible rendering of text that may be empty or contain line breaks. */
  const show = (s: string) => (s === '' ? '∅' : s.replace(/\n/g, '↵').replace(/\t/g, '⇥'));
</script>

{#if !ran}
  <EmptyState message={hasText ? 'Waiting for a valid pattern…' : 'Type a pattern, then some text to test it on.'} />
{:else if matches.length === 0}
  <EmptyState message="No match in this text." />
{:else}
  <div class="mt" role="table" aria-label="Matches">
    {#each visible as m, i (i)}
      <div class="mt-match" role="rowgroup">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="mt-row" role="row" onmouseenter={() => onhover?.({ from: m.start, to: m.end })} onmouseleave={() => onhover?.(null)}>
          <span class="mt-i">{i + 1}</span>
          <code class="mt-text">{show(m.groups[0]?.text ?? '')}</code>
          <span class="mt-span">{m.start}–{m.end}</span>
        </div>
        {#each m.groups as g, n (n)}
          {#if n > 0}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="mt-row mt-group" class:unset={!g} role="row"
              onmouseenter={() => g && onhover?.({ from: g.start, to: g.end })} onmouseleave={() => onhover?.(null)}>
              <span class="mt-chip" style:--c="var(--rx-g{((n - 1) % 6) + 1})">{groupNames[n] ?? n}</span>
              <code class="mt-text">{g ? show(g.text) : 'did not participate'}</code>
              {#if g}<span class="mt-span">{g.start}–{g.end}</span>{/if}
            </div>
          {/if}
        {/each}
      </div>
    {/each}
  </div>
  {#if matches.length > SHOWN || truncated}
    <p class="mt-more">Showing the first {SHOWN} of {truncated ? 'at least ' : ''}{matches.length} matches.</p>
  {/if}
{/if}

<style>
  .mt { display: flex; flex-direction: column; font-size: var(--font-size-sm); }
  .mt-match { border-bottom: 1px solid var(--border-subtle); padding: 3px 0; }
  .mt-row {
    display: grid;
    grid-template-columns: 78px minmax(0, 1fr) auto;
    align-items: baseline;
    gap: 8px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }
  .mt-row:hover { background: var(--bg-hover); }
  .mt-i { color: var(--text-muted); font-variant-numeric: tabular-nums; text-align: right; }
  .mt-text {
    font-family: var(--font-code);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mt-span { font-size: var(--font-size-xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .mt-group { padding-left: 6px; font-size: var(--font-size-xs); }
  .mt-group.unset { opacity: 0.5; }
  .mt-chip {
    justify-self: end;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 6px;
    border-radius: 999px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    background: color-mix(in srgb, var(--c) 45%, transparent);
    border: 1px solid var(--c);
  }
  .mt-more { margin: 8px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
