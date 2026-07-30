<script lang="ts">
  /**
   * DragOverlay — the contents of the shared drag-ghost overlay window
   * (`drag-overlay` label). A transparent, click-through, always-on-top window
   * that follows the cursor during a cross-window drag, so the ghost stays
   * visible even when the pointer leaves the source explorer window (a DOM-only
   * ghost is clipped to its own webview).
   *
   * The label text is pulled on mount (a drag may already be in flight when the
   * window is first built) and refreshed on the `arbor://drag-overlay-set`
   * event for every subsequent drag (the window is reused, not recreated).
   *
   * Backed by an explicit transparent background so only the pill paints — the
   * window itself is invisible.
   */
  import { onMount, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { Files } from 'lucide-svelte';
  import { getDragOverlayText } from '$lib/ipc/fs';

  let label = $state('');
  let un: UnlistenFn | null = null;

  onMount(async () => {
    try { label = await getDragOverlayText(); } catch { /* ignore */ }
    try { un = await listen<string>('arbor://drag-overlay-set', e => { label = e.payload; }); }
    catch { /* ignore */ }
  });
  onDestroy(() => un?.());
</script>

<div class="drag-ghost">
  <Files size={13} />
  <span>{label}</span>
</div>

<style>
  /* The window is transparent — paint nothing but the pill. */
  :global(html), :global(body) { background: transparent !important; overflow: hidden; }

  .drag-ghost {
    /* Anchored top-left (NOT centred): the window's top-left corner is what we
       place at the cursor, so the pill must hug that corner — a small inset
       leaves room for the shadow on the transparent window. */
    position: fixed;
    top: 4px;
    left: 4px;
    width: max-content;
    height: max-content;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: calc(100vw - 8px);
    padding: 6px 11px;
    border-radius: 8px;
    background: var(--accent, #4c8dff);
    color: #fff;
    font-size: var(--font-size-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
    pointer-events: none;
  }
  .drag-ghost span { overflow: hidden; text-overflow: ellipsis; }
</style>
