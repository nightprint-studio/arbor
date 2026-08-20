<script lang="ts">
  /**
   * The files the project no longer has.
   *
   * This is the scope that exists because the alternative is awful: a deleted file has no
   * row in any tree to right-click, so without a list of its own the only way to its
   * history is through the history of the folder it used to be in — which means
   * remembering the folder, and recognising the file inside somebody else's change set.
   *
   * So: flat, searchable, newest loss first, path always visible (two files called
   * `mod.rs` are told apart by nothing else), and each row says how it went — a delete
   * and a move look identical from the point of view of "it is not where I left it", and
   * the difference decides what you do next.
   */
  import { Trash2, CornerUpRight } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { formatAgo, formatBytes } from '$lib/utils/format';
  import type { DeletedEntry } from '$lib/ipc/bennu/history';

  let {
    entries,
    filter = '',
    selectedPath = null,
    onSelect,
  }: {
    entries: DeletedEntry[];
    filter?: string;
    selectedPath?: string | null;
    onSelect: (entry: DeletedEntry) => void;
  } = $props();

  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => e.path.toLowerCase().includes(q));
  });

  /** A move keeps its content and its history under the new name; a delete does not.
   *  The title the backend wrote says which, so the row shows it rather than guessing. */
  function movedTo(entry: DeletedEntry): string | null {
    const m = entry.title?.match(/^Moved to (.+)$/);
    return m ? m[1] : null;
  }

  function dir(path: string): string {
    const at = path.lastIndexOf('/');
    return at < 0 ? '' : path.slice(0, at + 1);
  }
</script>

{#if entries.length === 0}
  <div class="hd-empty">
    <EmptyState message="Nothing has been deleted in the retention window." />
  </div>
{:else if shown.length === 0}
  <div class="hd-empty"><EmptyState message="No deleted file matches." /></div>
{:else}
  <ul class="hd" role="listbox" aria-label="Deleted files">
    {#each shown as e (e.path + e.at)}
      {@const moved = movedTo(e)}
      <li role="option" aria-selected={e.path === selectedPath}>
        <button type="button" class="hd-row" class:on={e.path === selectedPath} onclick={() => onSelect(e)}>
          <span class="hd-ic" aria-hidden="true">
            {#if moved}<CornerUpRight size={13} />{:else}<Trash2 size={13} />{/if}
          </span>
          <span class="hd-main">
            <span class="hd-name">{e.name}</span>
            <span class="hd-path">{dir(e.path)}</span>
            {#if moved}<span class="hd-moved">→ {moved}</span>{/if}
          </span>
          <span class="hd-when">
            {formatAgo(e.at)}
            <span class="hd-sub">{e.size ? formatBytes(e.size) : 'no content kept'}</span>
          </span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .hd-empty { display: flex; align-items: center; justify-content: center; height: 100%; padding: 14px; }

  .hd { list-style: none; margin: 0; padding: 4px 0 8px; height: 100%; overflow-y: auto; }

  .hd-row {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 7px 12px; text-align: left;
    background: none; border: 0; border-left: 2px solid transparent;
    color: inherit; cursor: pointer; font: inherit;
  }
  .hd-row:hover { background: var(--bg-hover); }
  .hd-row.on { background: var(--bg-selected, var(--bg-hover)); border-left-color: var(--accent); }

  .hd-ic { display: flex; flex: none; color: var(--text-faint); }
  .hd-main { min-width: 0; flex: 1; }

  /* Struck through, but only the name: the path is how you tell two `mod.rs` apart, and
     a struck-through path is a path you have to squint at. */
  .hd-name {
    display: block; font-size: var(--font-size-sm); color: var(--text-secondary);
    text-decoration: line-through; text-decoration-color: var(--text-faint);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hd-path, .hd-moved {
    display: block; font-family: var(--font-code); font-size: var(--font-size-2xs);
    color: var(--text-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hd-moved { color: var(--color-tag, #c792ea); }

  .hd-when {
    flex: none; text-align: right;
    font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap;
  }
  .hd-sub { display: block; font-size: var(--font-size-2xs); color: var(--text-faint); }
</style>
