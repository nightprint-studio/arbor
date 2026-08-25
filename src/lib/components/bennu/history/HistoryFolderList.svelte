<script lang="ts">
  /**
   * What is in this folder, including what is not any more.
   *
   * The column is a **merge** of two sources, and it has to be: the history knows about
   * files that were deleted (which the tree cannot show) and the tree knows about files
   * nobody ever edited (which the history has never heard of). Showing either alone would
   * be a folder listing that is quietly wrong in one direction.
   *
   * A ghost row — struck through, with a Restore beside it — is the whole point of the
   * folder scope: you look for what disappeared where you last saw it, not in a global
   * list of everything that ever went.
   *
   * Rows belonging to the operation selected in the timeline are marked, so "what did
   * this refactor touch" is answered by looking rather than by clicking through six files.
   */
  import { RotateCcw, Folder } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconifyIconView from '@iconify/svelte';
  import { getFileIcon } from '$lib/utils/file-icons';
  import { formatAgo } from '$lib/utils/format';
  import { tooltip } from '$lib/actions/tooltip';

  /** One merged row. `tracked` is false for a file the history has never recorded — it
   *  is shown, but there is nothing to open for it yet. */
  export interface FolderRow {
    name: string;
    /** Absolute path. */
    path: string;
    isDir: boolean;
    deleted: boolean;
    tracked: boolean;
    at: number;
    /** In the operation currently selected in the timeline. */
    inChange: boolean;
  }

  let {
    rows,
    filter = '',
    onOpen,
    onRestore,
  }: {
    rows: FolderRow[];
    filter?: string;
    /** Drill down: show this file's own history. */
    onOpen: (row: FolderRow) => void;
    /** Put a deleted file back where it was. */
    onRestore: (row: FolderRow) => void;
  } = $props();

  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return q ? rows.filter((r) => r.name.toLowerCase().includes(q)) : rows;
  });
</script>

{#if shown.length === 0}
  <div class="hf-empty"><EmptyState message="Nothing here." /></div>
{:else}
  <ul class="hf" aria-label="Folder contents">
    {#each shown as row (row.path)}
      <li class="hf-li" class:in-change={row.inChange}>
        <button
          type="button"
          class="hf-row"
          class:gone={row.deleted}
          disabled={row.isDir}
          onclick={() => onOpen(row)}
        >
          <span class="hf-ic" aria-hidden="true">
            {#if row.isDir}
              <Folder size={13} />
            {:else}
              <IconifyIconView icon={getFileIcon(row.name)} width={14} height={14} />
            {/if}
          </span>
          <span class="hf-name">{row.name}</span>
          {#if row.deleted}
            <span class="hf-when">{formatAgo(row.at)}</span>
          {:else if !row.tracked}
            <span class="hf-when hf-untracked">no history</span>
          {/if}
        </button>
        {#if row.deleted && !row.isDir}
          <button
            type="button"
            class="hf-restore"
            use:tooltip={'Put it back where it was'}
            onclick={() => onRestore(row)}
          >
            <RotateCcw size={11} /> Restore
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .hf-empty { display: flex; align-items: center; justify-content: center; height: 100%; padding: 14px; }

  .hf { list-style: none; margin: 0; padding: 4px 0 8px; height: 100%; overflow-y: auto; }

  .hf-li { display: flex; align-items: center; }
  .hf-li:hover { background: var(--bg-hover); }
  /* The mark for "this operation touched it". A left rule rather than a background, so it
     survives the hover and the two never argue about which colour the row is. */
  .hf-li.in-change { box-shadow: inset 2px 0 0 var(--accent); }

  .hf-row {
    display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1;
    padding: 5px 6px 5px 12px; text-align: left;
    background: none; border: 0; color: inherit; cursor: pointer; font: inherit;
  }
  .hf-row:disabled { cursor: default; }

  .hf-ic { display: flex; flex: none; color: var(--text-muted); }
  .hf-name {
    font-size: var(--font-size-sm); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .hf-row.gone .hf-name {
    text-decoration: line-through; text-decoration-color: var(--text-faint);
    color: var(--text-faint);
  }
  .hf-row.gone .hf-ic { opacity: 0.45; }

  .hf-when {
    margin-left: auto; flex: none;
    font-size: var(--font-size-2xs); color: var(--text-faint); white-space: nowrap;
  }
  .hf-untracked { font-style: italic; }

  .hf-restore {
    flex: none; display: inline-flex; align-items: center; gap: 4px;
    margin-right: 8px; height: 20px; padding: 0 8px;
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-muted);
    font-size: var(--font-size-2xs); cursor: pointer;
  }
  .hf-restore:hover { color: var(--text-primary); border-color: var(--accent); }
</style>
