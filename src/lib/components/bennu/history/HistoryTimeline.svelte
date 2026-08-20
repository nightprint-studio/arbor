<script lang="ts">
  /**
   * The Local History timeline: when things happened, and what kind of thing each was.
   *
   * Rows are grouped under a day heading rather than each carrying a full date — you read
   * this column top to bottom looking for a moment, and "Today / 14:32" is how a moment is
   * remembered. The kind chip is the other half: it is what tells you whether to look at a
   * row at all, because a save is you and a refactor is a tool that may have touched a
   * dozen files at once.
   *
   * Keyboard-first: ↑/↓ move the selection and Enter confirms it, so the whole dialog is
   * reachable without the mouse (the diff follows the selection, so the arrows already
   * show you what each row is).
   */
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { clockTime, dayLabel, formatBytes } from '$lib/utils/format';
  import { kindMeta, revisionTitle } from './kind';
  import type { TimelineRow } from './timeline-rows';

  let {
    rows,
    selectedId = null,
    onSelect,
    emptyMessage = 'Nothing recorded yet.',
  }: {
    rows: TimelineRow[];
    selectedId?: string | null;
    onSelect: (row: TimelineRow) => void;
    emptyMessage?: string;
  } = $props();

  /** Rows with a day heading inserted wherever the calendar day changes. Computed once
   *  per list rather than compared per row, so the heading logic lives in one place. */
  const withDays = $derived.by(() => {
    const out: { day?: string; row?: TimelineRow }[] = [];
    let current = '';
    for (const row of rows) {
      const day = dayLabel(row.at);
      if (day !== current) {
        current = day;
        out.push({ day });
      }
      out.push({ row });
    }
    return out;
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const at = rows.findIndex((r) => r.id === selectedId);
    const next = e.key === 'ArrowDown' ? at + 1 : at - 1;
    const row = rows[next < 0 ? 0 : Math.min(next, rows.length - 1)];
    if (row) onSelect(row);
  }
</script>

{#if rows.length === 0}
  <div class="ht-empty"><EmptyState message={emptyMessage} /></div>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <ul class="ht" role="listbox" aria-label="Revisions" tabindex="0" onkeydown={onKeydown}>
    {#each withDays as item, i (i)}
      {#if item.day}
        <li class="ht-day" role="presentation">{item.day}</li>
      {:else if item.row}
        {@const row = item.row}
        {@const meta = kindMeta(row.kind)}
        <li role="option" aria-selected={row.id === selectedId}>
          <button
            type="button"
            class="ht-row"
            class:on={row.id === selectedId}
            onclick={() => onSelect(row)}
          >
            <span class="ht-time">{clockTime(row.at)}</span>
            <span class="ht-main">
              <span class="ht-title">
                {revisionTitle(row.kind, row.title)}
                <span class="ht-chip tone-{meta.tone}">{meta.label}</span>
                {#if row.label}<span class="ht-chip tone-warning" title="Labelled — never expires">🏷 {row.label}</span>{/if}
              </span>
              <span class="ht-meta">
                {#if row.files > 1}{row.files} files{:else if row.size}{formatBytes(row.size)}{:else}—{/if}
              </span>
            </span>
          </button>
        </li>
      {/if}
    {/each}
  </ul>
{/if}

<style>
  .ht-empty { display: flex; align-items: center; justify-content: center; height: 100%; padding: 12px; }

  .ht {
    list-style: none; margin: 0; padding: 0 0 8px;
    height: 100%; overflow-y: auto;
  }
  .ht:focus-visible { outline: none; box-shadow: inset 0 0 0 1px var(--accent); }

  .ht-day {
    padding: 9px 12px 3px;
    font-size: var(--font-size-2xs); letter-spacing: 0.05em; text-transform: uppercase;
    color: var(--text-faint);
    position: sticky; top: 0; background: var(--bg-base); z-index: 1;
  }

  .ht-row {
    display: flex; gap: 9px; align-items: flex-start;
    width: 100%; padding: 6px 12px; text-align: left;
    background: none; border: 0; border-left: 2px solid transparent;
    color: inherit; cursor: pointer; font: inherit;
  }
  .ht-row:hover { background: var(--bg-hover); }
  .ht-row.on { background: var(--bg-selected, var(--bg-hover)); border-left-color: var(--accent); }

  .ht-time {
    flex: none; width: 40px; padding-top: 1px;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .ht-main { min-width: 0; flex: 1; }
  .ht-title {
    display: flex; align-items: center; gap: 7px; flex-wrap: wrap;
    font-size: var(--font-size-sm); color: var(--text-primary);
  }
  .ht-meta { display: block; font-size: var(--font-size-xs); color: var(--text-faint); margin-top: 1px; }

  .ht-chip {
    font-size: var(--font-size-2xs); line-height: 15px; padding: 0 6px;
    border: 1px solid currentColor; border-radius: 9px; white-space: nowrap;
  }
  .tone-muted   { color: var(--text-muted); }
  .tone-accent  { color: var(--accent); }
  .tone-warning { color: var(--warning); }
  .tone-tag     { color: var(--color-tag, #c792ea); }
  .tone-error   { color: var(--error); }
  .tone-success { color: var(--success); }
</style>
