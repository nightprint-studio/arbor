<script lang="ts">
  /**
   * The commits of a Git history, newest first: subject, who, when, and the short id.
   *
   * Keyboard-first like the Local History timeline: ↑/↓ move the selection, and the diff beside
   * it follows, so the arrows alone walk the whole history.
   */
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { FileHistoryEntry } from '$lib/ipc/corvus/fileHistory';

  let {
    entries,
    selected = null,
    onSelect,
    emptyMessage = 'No commits touched this.',
  }: {
    entries: readonly FileHistoryEntry[];
    selected?: string | null;
    onSelect: (commit: string) => void;
    emptyMessage?: string;
  } = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const at = entries.findIndex((c) => c.commit === selected);
    const next = e.key === 'ArrowDown' ? at + 1 : at - 1;
    const entry = entries[next < 0 ? 0 : Math.min(next, entries.length - 1)];
    if (entry) onSelect(entry.commit);
  }
</script>

{#if entries.length === 0}
  <div class="gc-empty"><EmptyState message={emptyMessage} /></div>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <ul class="gc" role="listbox" aria-label="Commits" tabindex="0" onkeydown={onKeydown}>
    {#each entries as c (c.commit)}
      <li role="option" aria-selected={c.commit === selected}>
        <button type="button" class="gc-row" class:on={c.commit === selected} onclick={() => onSelect(c.commit)}>
          <span class="gc-summary">{c.summary || '(no message)'}</span>
          <span class="gc-meta">
            <span class="gc-sha">{c.short}</span>
            <span class="gc-who">{c.author}</span>
            <span class="gc-date">{c.date}</span>
          </span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .gc-empty { display: flex; align-items: center; justify-content: center; height: 100%; padding: 12px; }
  .gc { list-style: none; margin: 0; padding: 0 0 8px; height: 100%; overflow-y: auto; }
  .gc:focus-visible { outline: none; box-shadow: inset 0 0 0 1px var(--accent); }

  .gc-row {
    display: flex; flex-direction: column; gap: 2px;
    width: 100%; padding: 6px 12px; text-align: left;
    background: none; border: 0; border-left: 2px solid transparent;
    color: inherit; cursor: pointer; font: inherit;
  }
  .gc-row:hover { background: var(--bg-hover); }
  .gc-row.on { background: var(--bg-selected, var(--bg-hover)); border-left-color: var(--accent); }

  .gc-summary {
    font-size: var(--font-size-sm); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .gc-meta { display: flex; gap: 8px; font-size: var(--font-size-xs); color: var(--text-faint); }
  .gc-sha  { font-family: var(--font-code); color: var(--color-tag); }
  .gc-who  { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .gc-date { margin-left: auto; flex: none; }
</style>
