<script lang="ts">
  /**
   * Where each column of the result actually comes from.
   *
   * ## One row per column, one chain per row
   *
   * The chain is the answer, not its endpoint. `CODSA ← V_TIPI.CENINT ←
   * TAB_TIPI.CENINT` says *which view renamed it*, which is half of what somebody
   * asking this question wants — and a bare "TAB_TIPI" would hide it.
   *
   * ## It never lets a deduction look like a fact
   *
   * The band at the top says what this is, in words, before any name is read. It is
   * the same rule the plan panel follows for estimate-versus-measurement, and for
   * the same reason: the two look identical once you are reading names, and the
   * difference is what decides whether you may write to what you found.
   */
  import { Spline, TriangleAlert } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { baseColumn, renamed, type Trace } from '$lib/ipc/picus/lineage';
  import type { TabLineage } from '$lib/stores/picus/lineage.svelte';

  interface Props {
    state: TabLineage;
    /** Nothing to trace — no statement, or no open session. */
    disabled: boolean;
    /** Why it is unavailable, when it is. */
    reason: string;
    onTrace: () => void;
  }

  let { state, disabled, reason, onTrace }: Props = $props();

  const lineage = $derived(state.lineage);

  /** The chain as one line of text, for the copy button and the tooltip. */
  function chainOf(trace: Trace): string {
    const steps = trace.hops.map((h) => `${h.relation}.${h.column}`);
    return [trace.output, ...steps].join('  ←  ');
  }

  /** Everything traced, as text — for pasting into a ticket. */
  const asText = $derived(
    (lineage?.columns ?? [])
      .map((trace) => {
        if (trace.verdict === 'resolved') return chainOf(trace);
        const from = trace.reads.map((r) => `${r.relation}.${r.column}`).join(', ');
        if (trace.verdict === 'derived') {
          return `${trace.output}  =  computed from ${from || 'nothing nameable'}`;
        }
        // The wording carries the distinction into the clipboard too: this text ends
        // up in tickets, where "computed" and "one of these per row" lead to
        // different decisions.
        if (trace.verdict === 'split') {
          return `${trace.output}  =  one of ${from}, depending on the row`;
        }
        return `${trace.output}  —  ${trace.stopped}`;
      })
      .join('\n'),
  );

  const counts = $derived({
    resolved: (lineage?.columns ?? []).filter((t) => t.verdict === 'resolved').length,
    derived: (lineage?.columns ?? []).filter((t) => t.verdict === 'derived').length,
    split: (lineage?.columns ?? []).filter((t) => t.verdict === 'split').length,
    unresolved: (lineage?.columns ?? []).filter((t) => t.verdict === 'unresolved').length,
  });
</script>

<div class="ln">
  <!-- What this is, said before any name is read. -->
  <div class="ln-kind">
    <Spline size={14} />
    <strong>Deduced</strong>
    <span class="ln-note">
      Read from the views' own SQL, not reported by the server — so it can be wrong
      where the server's answer cannot.
    </span>
    <span class="ln-spacer"></span>
    {#if lineage}
      <span class="ln-tally" use:tooltip={'Columns that reach a table, are computed, or could not be followed.'}>
        {counts.resolved} traced
        {#if counts.derived}· {counts.derived} computed{/if}
        {#if counts.split}· {counts.split} split{/if}
        {#if counts.unresolved}· {counts.unresolved} stopped{/if}
      </span>
      <CopyButton value={asText} title="Copy every chain as text" toastSuccess="Lineage copied." />
    {/if}
    <Button
      variant="secondary"
      size="xs"
      disabled={disabled || state.running}
      tooltip={disabled ? reason : 'Follow every column back through the views to the table it is read from.'}
      onclick={onTrace}
    >
      {lineage ? 'Trace again' : 'Trace columns'}
    </Button>
  </div>

  {#if lineage?.through.length}
    <!-- The stack that was walked. Worth showing on its own: with views on views it
         is often the first surprise, and it is what the chains below are made of. -->
    <div class="ln-through">
      through {lineage.through.join(' › ')}
    </div>
  {/if}

  {#if state.running}
    <StateBlock tone="loading">
      {#snippet spinner()}<Spinner size={14} />{/snippet}
      <span>Reading the views and following each column…</span>
    </StateBlock>
  {:else if state.error}
    <StateBlock tone="error" label={state.error} />
  {:else if !lineage}
    <StateBlock
      tone="info"
      label="Trace follows each column of this result back through the views it passes through, to the table it is read from."
    />
  {:else if !lineage.columns.length}
    <StateBlock tone="info" label="This statement projects nothing that can be traced." />
  {:else}
    <ul class="ln-rows">
      {#each lineage.columns as trace, i (i)}
        <li class="ln-row">
          <div class="ln-head">
            <span class="ln-out" class:ln-renamed={renamed(trace)}>{trace.output}</span>
            {#if trace.verdict === 'resolved'}
              {#if renamed(trace)}
                <Badge
                  variant="tone"
                  tone="info"
                  size="sm"
                  label={`renamed from ${baseColumn(trace)}`}
                />
              {/if}
            {:else if trace.verdict === 'derived'}
              <Badge variant="tone" tone="warning" size="sm" label="computed" />
            {:else if trace.verdict === 'split'}
              <Badge variant="tone" tone="info" size="sm" label="one of several" />
            {:else}
              <Badge variant="tone" tone="neutral" size="sm" label="not followed" />
            {/if}
          </div>

          {#if trace.verdict === 'resolved'}
            <div class="ln-chain" use:tooltip={chainOf(trace)}>
              {#each trace.hops as hop, h (h)}
                <span class="ln-arrow" aria-hidden="true">←</span>
                <span class="ln-hop" class:ln-hop-table={!hop.isView}>
                  <span class="ln-rel">{hop.relation}</span><span class="ln-dot">.</span
                  ><span class="ln-col">{hop.column}</span>
                </span>
              {/each}
            </div>
          {:else if trace.verdict === 'derived' || trace.verdict === 'split'}
            <!-- The two are one branch because they render the same list, and two
                 sentences because they mean opposite things about writing. Saying
                 "computed, nothing to write back through" of a UNION would be false
                 twice: there is a real column behind every row, and two writable
                 tables rather than none. -->
            <p class="ln-why">
              {#if trace.verdict === 'split'}
                One of these, depending on the row — a set operation reads a different
                table for different rows, and which one a given row came from is not
                in the result.
              {:else}
                Computed — there is no single column behind it{trace.reads.length
                  ? ', and nothing to write back through.'
                  : '.'}
              {/if}
            </p>
            {#if trace.reads.length}
              <div class="ln-chain">
                {#each trace.reads as read, r (r)}
                  <span class="ln-hop ln-hop-read">
                    <span class="ln-rel">{read.relation || '?'}</span><span class="ln-dot">.</span
                    ><span class="ln-col">{read.column}</span>
                  </span>
                {/each}
              </div>
            {/if}
          {:else}
            <p class="ln-why ln-stopped">
              <TriangleAlert size={11} />
              {trace.stopped}
            </p>
          {/if}
        </li>
      {/each}
    </ul>

    {#if lineage.truncated}
      <p class="ln-foot">
        Some trails were deeper than Picus follows and stop part-way. What is shown is
        true as far as it goes.
      </p>
    {/if}
  {/if}
</div>

<style>
  .ln { display: flex; flex-direction: column; min-height: 0; width: 100%; }
  .ln-spacer { flex: 1; min-width: 8px; }

  /* The band that says this is a deduction. Deliberately the loudest thing here —
     everything under it is table names, which look the same either way. */
  .ln-kind {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    padding: 5px 10px 5px 12px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    color: var(--warning);
    font-size: var(--font-size-xs);
  }
  .ln-kind strong {
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: var(--font-size-2xs);
  }
  .ln-note {
    color: var(--text-muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ln-tally {
    flex-shrink: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    cursor: help;
    white-space: nowrap;
  }

  .ln-through {
    flex-shrink: 0;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border-subtle);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ln-rows { flex: 1; min-height: 0; overflow: auto; list-style: none; margin: 0; padding: 4px 0; }

  .ln-row { padding: 3px 12px; }
  .ln-row + .ln-row {
    border-top: 1px solid color-mix(in srgb, var(--border-subtle) 45%, transparent);
  }

  .ln-head { display: flex; align-items: center; gap: 8px; min-height: 20px; }
  .ln-out {
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    white-space: nowrap;
  }
  /* A renamed column is the interesting case, so it is the one that is marked. */
  .ln-renamed { color: var(--info); }

  .ln-chain {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
    padding: 1px 0 2px 14px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
  }
  .ln-arrow { color: var(--text-disabled); }
  .ln-hop { color: var(--text-muted); white-space: nowrap; }
  /* The end of the trail — the answer people came for, so it is the one in colour. */
  .ln-hop-table { color: var(--accent); }
  .ln-hop-read { color: var(--text-muted); }
  .ln-rel { font-weight: 600; }
  .ln-dot { color: var(--text-disabled); }

  .ln-why {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 2px 0 2px 14px;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .ln-stopped { color: var(--text-disabled); }

  .ln-foot {
    flex-shrink: 0;
    margin: 0;
    padding: 5px 12px;
    border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
  }
</style>
