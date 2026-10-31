<script lang="ts">
  /**
   * The files attached to a message — above the text in the composer (each removable) and above the bubble in
   * a sent message (read-only). A picture is a thumbnail that opens in the lightbox, with its neighbours a
   * key away; a text file is a chip with its name and size.
   *
   * Removal is by keyboard too: every ✕ is a button in the tab order, and the composer also takes Backspace on
   * an empty field (see `AssistantComposer`).
   */
  import { FileText, X } from 'lucide-svelte';
  import AssistantImage from './AssistantImage.svelte';
  import type { Picture } from './assistant-images';
  import type { AttachmentItem } from './assistant-types';

  let { items, onRemove }: { items: AttachmentItem[]; onRemove?: (index: number) => void } = $props();

  const gallery = $derived<Picture[]>(items.filter((i) => i.kind === 'image' && i.path).map((i) => ({ path: i.path!, caption: i.name })));

  function size(bytes: number): string {
    if (bytes < 1000) return `${bytes} B`;
    return bytes < 1_000_000 ? `${Math.round(bytes / 1000)} KB` : `${(bytes / 1_000_000).toFixed(1)} MB`;
  }
</script>

<div class="atts">
  {#each items as item, i (i + item.name)}
    <div class="att" class:picture={item.kind === 'image'}>
      {#if item.kind === 'image'}
        <AssistantImage small path={item.path ?? ''} src={item.preview ?? null} caption={item.name} {gallery} />
        <span class="name" title={item.name}>{item.name}</span>
      {:else}
        <span class="file" title={`${item.name} · ${size(item.size)}`}>
          <FileText size={12} /><span class="name">{item.name}</span><span class="size">{size(item.size)}</span>
        </span>
      {/if}
      {#if onRemove}
        <button type="button" class="rm" aria-label={`Remove ${item.name}`} onclick={() => onRemove(i)}><X size={10} /></button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .atts { display: flex; flex-wrap: wrap; gap: 6px; min-width: 0; }
  .att { position: relative; display: flex; flex-direction: column; gap: 3px; max-width: 150px; }
  .picture { width: 58px; }
  .name { font-size: var(--font-size-2xs); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file {
    display: inline-flex; align-items: center; gap: 6px; padding: 4px 8px;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    background: var(--bg-overlay); color: var(--text-secondary); min-width: 0;
  }
  .size { color: var(--text-muted); font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; flex-shrink: 0; }
  .rm {
    position: absolute; top: -5px; right: -5px; display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; padding: 0; border-radius: 50%;
    border: 1px solid var(--border); background: var(--bg-elevated); color: var(--text-secondary); cursor: pointer;
  }
  .rm:hover, .rm:focus-visible { color: var(--danger); border-color: var(--danger); outline: none; }
</style>
