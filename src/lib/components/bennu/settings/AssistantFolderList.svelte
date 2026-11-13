<script lang="ts">
  /**
   * A list of folders picked by hand, with "Add folder…" (the Sitta explorer as a picker) and a remove button
   * per row. The Imber settings keep two of these — the folders it may work in, and the folders of skills —
   * so the look and the keyboard path are the same for both.
   */
  import { FolderOpen, Plus, X } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';

  let {
    label,
    paths,
    empty,
    pickerTitle,
    onadd,
    onremove,
  }: {
    label: string;
    paths: string[];
    /** Shown when the list is empty. */
    empty: string;
    /** The picker's title. */
    pickerTitle: string;
    onadd: (path: string) => void;
    onremove: (path: string) => void;
  } = $props();

  let picking = $state(false);
</script>

<div class="folders">
  <div class="folders-head">
    <span><FolderOpen size={12} /> {label}</span>
    <Button variant="ghost" size="xs" onclick={() => (picking = true)}>
      <Plus size={11} /> Add folder…
    </Button>
  </div>
  {#each paths as f (f)}
    <div class="folder">
      <span class="path" title={f}>{f}</span>
      <IconButton tooltip="Remove" size={20} onclick={() => onremove(f)}><X size={11} /></IconButton>
    </div>
  {:else}
    <p class="empty">{empty}</p>
  {/each}
</div>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title={pickerTitle}
    onConfirm={(path) => { onadd(path); picking = false; }}
    onCancel={() => (picking = false)}
  />
{/if}

<style>
  .folders { display: flex; flex-direction: column; gap: 4px; padding: 10px 14px 12px; }
  .folders-head { display: flex; align-items: center; justify-content: space-between; font-size: var(--font-size-xs); color: var(--text-secondary); }
  .folders-head span { display: inline-flex; align-items: center; gap: 6px; }
  .folder {
    display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 4px 6px 4px 10px; border-radius: var(--radius-sm);
    background: var(--bg-overlay);
  }
  .path { min-width: 0; overflow-wrap: anywhere; font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .empty { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
