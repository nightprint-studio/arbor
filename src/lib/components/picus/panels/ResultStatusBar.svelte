<script lang="ts">
  /**
   * What happened, in the header — the counts, the time, and whether the rows can
   * be written to.
   *
   * Four mutually exclusive readings of one statement, and which applies is decided
   * here rather than by the caller: still running, a result with rows, a write with
   * a count, or a bare elapsed time. Keeping the choice in one place is what stops
   * the header from ever showing two of them at once.
   */
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { formatElapsed, type QueryTabState } from '$lib/stores/picus/query.svelte';
  import { formatRowTotal, type PicusResult } from '$lib/stores/picus/result.svelte';
  import type { Editability } from '$lib/stores/picus/result-edit.svelte';

  interface Props {
    tabState: QueryTabState;
    result: PicusResult | null;
    editable: Editability;
  }

  let { tabState, result, editable }: Props = $props();

  /**
   * Everything the row counter cannot fit, for the pointer that asks.
   *
   * Three separate facts, and which of them apply depends on the result: the total
   * may be the planner's guess rather than a count, the window may still be
   * filling, and while it is, sorting and the per-column filters stand down —
   * over a window they would order and hide a fraction of the rows while looking
   * like they had done all of them.
   */
  const partialNote = $derived.by(() => {
    if (!result) return '';
    const lines = [
      result.approximate
        ? `Estimated by the planner${result.counting ? ' — counting the exact number now' : ''}.`
        : `${result.total.toLocaleString()} row(s) in the result.`,
    ];
    if (!result.complete) {
      lines.push(
        `${result.loaded.toLocaleString()} loaded; the rest arrives as you scroll.`,
        'Sorting and the per-column filters wait until the whole result is loaded.',
      );
      // The way out, said where the state is said. Scrolling to the end to get the
      // controls back is what you are reduced to without it, and someone reading
      // this line is exactly the person who wants to know there is a button.
      lines.push(
        result.loadingAll
          ? 'Loading the rest now.'
          : result.loadAll
            ? 'The button at the head of the filter row loads all of it at once.'
            : 'It is too large to hold whole — narrow it with a WHERE instead.',
      );
    }
    return lines.join(' ');
  });
</script>

{#if tabState.running}
  <span class="qr-stats"><Spinner size={11} /> running…</span>
{:else if result}
  <!-- The total is the server's ESTIMATE until the background count lands, and
       carries a `~` for exactly as long as that is true. Precision the product does
       not have must not be implied by the way it is printed.

       "Still filling" is said HERE, in four words, and not in the full-width notice
       this used to put above the grid. That notice was forty words of permanent
       chrome explaining the sorting rules to someone who had not asked to sort, and
       it cost a whole band of a panel that was already giving the rows less height
       than its own headers. The sentence is still available — it is the tooltip —
       and the count it was built around is now next to the count it qualifies. -->
  <span class="qr-stats" use:tooltip={partialNote}>
    {result.complete
      ? formatRowTotal(result)
      : `${result.loaded.toLocaleString()} of ${formatRowTotal(result)}`} rows
    · {formatElapsed(tabState.elapsedMs ?? result.elapsedMs)}
  </span>
{:else if tabState.affected !== null}
  <!-- A write has no result to read a time off, which is why the tab keeps one:
       "how long did that take" is asked about an UPDATE at least as often as about
       a SELECT. -->
  <span class="qr-stats">
    {tabState.affected.toLocaleString()} rows affected{tabState.elapsedMs !== null
      ? ` · ${formatElapsed(tabState.elapsedMs)}`
      : ''}
  </span>
{:else if tabState.elapsedMs !== null}
  <span class="qr-stats">{formatElapsed(tabState.elapsedMs)}</span>
{/if}

{#if result}
  <!-- Said before it is needed. "Can I change this?" is asked by double-clicking a
       cell, and a grid that simply did nothing would read as broken rather than as
       protecting a table with no key. -->
  <span
    class="qr-edit"
    class:qr-edit-on={editable.ok}
    use:tooltip={{
      content: editable.ok
        ? `Double-click a cell to change it. Keyed on ${editable.keys.join(', ')}. Nothing is written until you press Store.`
        : editable.reason,
    }}
  >
    {editable.ok ? 'editable' : 'read-only rows'}
  </span>
{/if}

<style>
  .qr-stats {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
  }

  /* Quiet by default and accented when it is an affordance: "read-only rows" is a
     fact, "editable" is an invitation, and they should not weigh the same. */
  .qr-edit {
    flex-shrink: 0;
    padding: 1px 6px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-disabled);
    cursor: help;
    white-space: nowrap;
  }
  .qr-edit-on {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }
</style>
