<script lang="ts">
  /**
   * RecordingsPanel — the captures library (right dock). Filter by name, rename
   * inline, view a capture (preview modal), reveal or delete. The header shows the
   * item count AND the total size on disk. Mocked: reveal toasts, the rest mutate
   * the in-memory list.
   */
  import { Video, Camera, Images, Trash2, FolderOpen, Eye, Pencil, Search, Grid2x2 } from 'lucide-svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { fly } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { cubicOut } from 'svelte/easing';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import TytoCapturePreview from '../TytoCapturePreview.svelte';
  import TytoThumb from '../TytoThumb.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { animStore } from '$lib/stores/animations.svelte';
  import { recorderStore, formatDuration } from '$lib/stores/tyto/recorder.svelte';
  import { formatBytes, formatAgo } from '$lib/utils/format';

  let query = $state('');
  let clearConfirmOpen = $state(false);
  let editingId = $state<string | null>(null);
  let previewId = $state<string | null>(null);

  const captures = $derived(recorderStore.captures);
  const filtered = $derived(
    captures.filter((c) => {
      const q = query.trim().toLowerCase();
      return !q || c.name.toLowerCase().includes(q) || c.target.toLowerCase().includes(q);
    }),
  );
  const previewCapture = $derived(captures.find((c) => c.id === previewId) ?? null);

  // A freshly-produced capture surfaces itself: the store latches its id and the shell
  // reveals this library; here we open its preview once (then clear the latch so closing
  // it stays closed). Works whether this panel was already open or just mounted.
  $effect(() => {
    const want = recorderStore.autoPreviewId;
    if (want && want !== previewId) {
      previewId = want;
      recorderStore.clearAutoPreview();
    }
  });

  function reveal(id: string) {
    void recorderStore.revealCapture(id);
  }
  function commitRename(id: string, name: string) {
    recorderStore.renameCapture(id, name);
    editingId = null;
  }
  function remove(id: string) {
    recorderStore.removeCapture(id);
    if (previewId === id) previewId = null;
  }
  function exportAtlas(id: string) {
    void recorderStore.exportAtlas(id);
  }
</script>

<div class="library">
  <header class="lib-header">
    <span class="lib-title">Library</span>
    <div class="lib-stats">
      <span class="stat"><b>{captures.length}</b> {captures.length === 1 ? 'item' : 'items'}</span>
      <span class="dot">·</span>
      <span class="stat size">{formatBytes(recorderStore.totalBytes)}</span>
    </div>
    {#if captures.length > 0}
      <button type="button" class="lib-clear" onclick={() => (clearConfirmOpen = true)} use:tooltip={'Delete every capture from disk'}>Delete all</button>
    {/if}
  </header>

  {#if captures.length > 0}
    <div class="lib-filter">
      <SearchBar bind:query showRegex={false} showCounter={false} placeholder="Filter captures…" ariaLabel="Filter captures" />
    </div>
  {/if}

  <div class="lib-list">
    {#if captures.length === 0}
      <StateBlock tone="neutral">
        {#snippet icon()}<Video size={26} />{/snippet}
        <div class="empty-title">No captures yet</div>
        <div class="empty-desc">Your recordings and screenshots will appear here.</div>
      </StateBlock>
    {:else if filtered.length === 0}
      <StateBlock tone="neutral">
        {#snippet icon()}<Search size={22} />{/snippet}
        <div class="empty-title">No matches</div>
        <div class="empty-desc">Nothing matches “{query}”.</div>
      </StateBlock>
    {:else}
      {#each filtered as cap (cap.id)}
        <div
          class="cap"
          class:flash={cap.id === recorderStore.captureFlashId}
          animate:flip={{ duration: animStore.dPanel, easing: cubicOut }}
          in:fly={{ y: -10, duration: animStore.dBase, easing: cubicOut }}
        >
          <button class="cap-thumb" onclick={() => (previewId = cap.id)} use:tooltip={'View'} aria-label={`View ${cap.name}`}>
            {#if cap.poster}
              <!-- A frame sequence is a directory, so it ships its own poster frame. -->
              <img class="cap-thumb-img" src={convertFileSrc(cap.poster)} alt={cap.name} loading="lazy" />
            {:else if cap.path && cap.kind === 'screenshot'}
              <img class="cap-thumb-img" src={convertFileSrc(cap.path)} alt={cap.name} loading="lazy" />
            {:else if cap.path && cap.kind === 'record'}
              <!-- A poster frame from the video itself (media fragment → first frame),
                   metadata-only so it stays cheap. -->
              <!-- svelte-ignore a11y_media_has_caption -->
              <video class="cap-thumb-img" src={`${convertFileSrc(cap.path)}#t=0.1`} preload="metadata" muted playsinline></video>
            {:else}
              <TytoThumb hue={cap.hue} kind={cap.kind === 'screenshot' ? 'screenshot' : 'record'} />
            {/if}
            {#if cap.durationMs}<span class="cap-dur">{formatDuration(cap.durationMs)}</span>{/if}
            {#if cap.kind === 'frames'}<span class="cap-tag">FRAMES</span>{/if}
            <span class="cap-thumb-hover"><Eye size={16} /></span>
          </button>

          <div class="cap-body">
            {#if editingId === cap.id}
              <InlineEdit
                value={cap.name}
                size="sm"
                onconfirm={(name) => commitRename(cap.id, name)}
                oncancel={() => (editingId = null)}
              />
            {:else}
              <div class="cap-name" title={cap.name}>{cap.name}</div>
              <div class="cap-meta">
                {#if cap.kind === 'record'}<Video size={11} />
                {:else if cap.kind === 'frames'}<Images size={11} />
                {:else}<Camera size={11} />{/if}
                {cap.target} · {formatBytes(cap.sizeBytes)} · {formatAgo(cap.createdAt)}
              </div>
            {/if}
          </div>

          {#if editingId !== cap.id}
            <div class="cap-actions">
              <button type="button" class="cap-act" use:tooltip={'View'} aria-label="View capture" onclick={() => (previewId = cap.id)}><Eye size={14} /></button>
              <button type="button" class="cap-act" use:tooltip={'Rename'} aria-label="Rename capture" onclick={() => (editingId = cap.id)}><Pencil size={13} /></button>
              {#if cap.kind === 'frames'}
                <button
                  type="button"
                  class="cap-act"
                  use:tooltip={'Export as sprite atlas'}
                  aria-label="Export as sprite atlas"
                  disabled={recorderStore.exportingId === cap.id}
                  onclick={() => exportAtlas(cap.id)}
                ><Grid2x2 size={14} /></button>
              {/if}
              <button type="button" class="cap-act" use:tooltip={'Reveal in folder'} aria-label="Reveal in folder" onclick={() => reveal(cap.id)}><FolderOpen size={14} /></button>
              <button type="button" class="cap-act danger" use:tooltip={'Delete'} aria-label="Delete capture" onclick={() => remove(cap.id)}><Trash2 size={14} /></button>
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if clearConfirmOpen}
  <ConfirmModal
    title="Delete every capture"
    message={`Permanently delete all ${captures.length} ${captures.length === 1 ? 'capture' : 'captures'}?`}
    detail="The files are removed from disk — recordings, screenshots and whole frame-sequence folders. This cannot be undone."
    variant="danger"
    confirmLabel="Delete all"
    onConfirm={() => { recorderStore.clearCaptures(); clearConfirmOpen = false; }}
    onCancel={() => (clearConfirmOpen = false)}
  />
{/if}

{#if previewCapture}
  <TytoCapturePreview
    capture={previewCapture}
    onClose={() => (previewId = null)}
    onReveal={() => reveal(previewCapture.id)}
    onDelete={() => remove(previewCapture.id)}
  />
{/if}

<style>
  .library { display: flex; flex-direction: column; height: 100%; width: 100%; min-width: 0; background: var(--bg-base); }

  .lib-header {
    display: flex; align-items: center; gap: 8px;
    height: 40px; flex-shrink: 0; padding: 0 12px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .lib-title { font-size: var(--font-size-sm); font-weight: 650; text-transform: uppercase; letter-spacing: 0.6px; color: var(--text-secondary); }
  .lib-stats {
    display: inline-flex; align-items: center; gap: 6px;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    background: var(--bg-input); border-radius: 999px; padding: 2px 9px;
  }
  .lib-stats b { color: var(--text-secondary); font-weight: 700; }
  .lib-stats .dot { color: var(--border); }
  .lib-stats .size { color: var(--accent); font-weight: 600; font-variant-numeric: tabular-nums; }
  .lib-clear {
    margin-left: auto; background: none; border: none; cursor: pointer;
    font-size: var(--font-size-xs); color: var(--text-muted);
    transition: color var(--transition-fast);
  }
  .lib-clear:hover { color: var(--error); }

  .lib-filter { flex-shrink: 0; padding: 8px 8px 4px; }

  .lib-list { flex: 1; overflow: auto; padding: 6px; display: flex; flex-direction: column; gap: 5px; }
  .empty-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-secondary); }
  .empty-desc { font-size: var(--font-size-xs); color: var(--text-muted); margin-top: 3px; max-width: 200px; }

  .cap {
    display: flex; align-items: center; gap: 11px;
    padding: 8px; border-radius: var(--radius-md);
    border: 1px solid transparent;
    transition: background var(--transition-fast), border-color var(--transition-fast), transform var(--transition-fast), box-shadow var(--transition-fast);
  }
  .cap:hover { background: var(--bg-hover); border-color: var(--border-subtle); transform: translateY(-1px); box-shadow: 0 4px 14px rgba(0,0,0,0.16); }

  /* Just-produced capture: a brief accent ring + glow so a fresh recording /
     screenshot reads as "new" the moment the library reveals it. */
  .cap.flash {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    background: var(--accent-subtle);
    animation: cap-flash 4s ease-out both;
  }
  @keyframes cap-flash {
    0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 55%, transparent); }
    18%  { box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 22%, transparent); }
    100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--accent) 0%, transparent); }
  }

  .cap-thumb {
    position: relative; flex-shrink: 0;
    width: 66px; height: 46px;
    display: block;
    border: none; padding: 0; cursor: pointer;
    border-radius: var(--radius-sm);
    color: #fff;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .cap-thumb-img { width: 100%; height: 100%; object-fit: cover; display: block; pointer-events: none; }
  /* Marks a capture that is a directory of stills rather than a single file — the
     thumbnail alone can't tell you, and reveal/delete behave differently. */
  .cap-tag {
    position: absolute; left: 3px; top: 3px;
    font-size: var(--font-size-3xs); font-weight: 700; letter-spacing: 0.4px;
    background: color-mix(in srgb, var(--accent) 88%, #000); color: #fff;
    padding: 0 4px; border-radius: 3px;
  }
  .cap-dur {
    position: absolute; right: 3px; bottom: 3px;
    font-size: var(--font-size-3xs); font-weight: 600; font-variant-numeric: tabular-nums;
    background: rgba(0, 0, 0, 0.55); color: #fff;
    padding: 0 3px; border-radius: 3px;
  }
  .cap-thumb-hover {
    position: absolute; inset: 0;
    display: flex; align-items: center; justify-content: center;
    background: rgba(0, 0, 0, 0.4);
    opacity: 0; transition: opacity var(--transition-fast);
  }
  .cap-thumb:hover .cap-thumb-hover { opacity: 1; }

  .cap-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; justify-content: center; }
  .cap-name { font-size: var(--font-size-sm); font-weight: 500; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .cap-meta {
    display: flex; align-items: center; gap: 5px;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .cap-meta :global(svg) { flex-shrink: 0; }

  .cap-actions { display: flex; gap: 1px; opacity: 0; transition: opacity var(--transition-fast); flex-shrink: 0; }
  .cap:hover .cap-actions { opacity: 1; }
  .cap-act {
    display: flex; align-items: center; justify-content: center;
    width: 26px; height: 26px; border: none; border-radius: var(--radius-sm);
    background: transparent; color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .cap-act:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .cap-act.danger:hover { color: var(--error); }
  /* Un export in corso: il bottone resta al suo posto (niente salto della fila) ma
     non si ripreme — riscrivere le stesse pagine due volte in parallelo è l'unico
     modo di ottenere un atlante a metà. */
  .cap-act:disabled { opacity: 0.45; cursor: default; pointer-events: none; }
</style>
