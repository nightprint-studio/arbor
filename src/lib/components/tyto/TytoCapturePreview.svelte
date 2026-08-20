<script lang="ts">
  /**
   * TytoCapturePreview — a capture's detail / preview modal: the media itself
   * (image, video, or a played frame sequence) plus its metadata and quick actions.
   * The stylized stand-in only shows when there is no file behind the entry (the
   * backend-down mock).
   */
  import { Video, Camera, Images, FolderOpen, Trash2, Clock, HardDrive, Crosshair, CalendarClock, Grid2x2 } from 'lucide-svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import TytoThumb from './TytoThumb.svelte';
  import TytoFramePlayer from './TytoFramePlayer.svelte';
  import { recorderStore, formatDuration, type Capture, type FrameSequence } from '$lib/stores/tyto/recorder.svelte';
  import { formatBytes } from '$lib/utils/format';

  let { capture, onClose, onReveal, onDelete }:
    { capture: Capture; onClose: () => void; onReveal: () => void; onDelete: () => void } = $props();

  const exporting = $derived(recorderStore.exportingId === capture.id);
  function exportAtlas() {
    if (!exporting) void recorderStore.exportAtlas(capture.id);
  }

  const created = $derived(new Date(capture.createdAt).toLocaleString());
  // The on-disk file as an asset URL (empty in the mock → falls back to the stylized
  // stand-in). The asset-protocol scope already covers png/mp4 (tauri.conf.json).
  const mediaUrl = $derived(capture.path ? convertFileSrc(capture.path) : '');

  const kindLabel = $derived(
    capture.kind === 'record' ? 'Recording' :
    capture.kind === 'frames' ? 'Frame sequence' : 'Screenshot',
  );

  // A frame sequence is a directory of images, so it is loaded (manifest + frame
  // list) rather than pointed at. `null` while loading, `false` once it failed.
  let sequence = $state<FrameSequence | null>(null);
  let sequenceFailed = $state(false);
  $effect(() => {
    const id = capture.kind === 'frames' ? capture.id : null;
    sequence = null;
    sequenceFailed = false;
    if (!id) return;
    void recorderStore.loadFrameSequence(id).then((seq) => {
      if (seq && seq.frames.length) sequence = seq;
      else sequenceFailed = true;
    });
  });
</script>

<!-- A sequence brings a transport bar the other kinds don't have; 600px squeezes it. -->
<Modal {onClose} width={capture.kind === 'frames' ? '720px' : '600px'} ariaLabel="Capture preview">
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if capture.kind === 'record'}<Video size={15} />
      {:else if capture.kind === 'frames'}<Images size={15} />
      {:else}<Camera size={15} />{/if}
      <span class="modal-title">{capture.name}</span>
      <span class="kind-badge">{kindLabel}</span>
    </ModalHeader>
  {/snippet}

  <div class="preview-body">
    {#if capture.kind === 'frames'}
      {#if sequence}
        <TytoFramePlayer {sequence} onExport={exportAtlas} />
      {:else if sequenceFailed}
        <div class="frame media">
          <StateBlock tone="error">
            {#snippet icon()}<Images size={22} />{/snippet}
            <div class="seq-title">This sequence can't be played</div>
            <div class="seq-desc">Its manifest is missing or unreadable — reveal it in the folder to inspect the frames.</div>
          </StateBlock>
        </div>
      {:else}
        <div class="frame media loading"><Spinner size={22} /></div>
      {/if}
    {:else}
    <div class="frame" class:media={!!mediaUrl}>
      {#if mediaUrl && capture.kind === 'record'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video class="media-el" src={mediaUrl} controls autoplay preload="auto"></video>
      {:else if mediaUrl}
        <img class="media-el" src={mediaUrl} alt={capture.name} />
      {:else}
        <TytoThumb hue={capture.hue} kind={capture.kind === 'screenshot' ? 'screenshot' : 'record'} />
        {#if capture.kind === 'record'}
          <span class="frame-dur">{formatDuration(capture.durationMs ?? 0)}</span>
        {/if}
        <span class="frame-note">Stylized preview · real frame comes with the backend</span>
      {/if}
    </div>
    {/if}

    <ul class="meta">
      <li><Crosshair size={13} /> <span>Target</span><b>{capture.target}</b></li>
      {#if capture.durationMs !== null}
        <li><Clock size={13} /> <span>Duration</span><b>{formatDuration(capture.durationMs)}</b></li>
      {/if}
      <li><HardDrive size={13} /> <span>Size</span><b>{formatBytes(capture.sizeBytes)}</b></li>
      <li><CalendarClock size={13} /> <span>Created</span><b>{created}</b></li>
    </ul>
  </div>

  {#snippet footer()}
    <Button variant="ghost" size="sm" onclick={onDelete}>
      {#snippet iconStart()}<Trash2 size={13} />{/snippet}
      Delete
    </Button>
    <div style="flex:1"></div>
    {#if capture.kind === 'frames'}
      <Button variant="secondary" size="sm" onclick={exportAtlas} loading={exporting}>
        {#snippet iconStart()}<Grid2x2 size={13} />{/snippet}
        {exporting ? 'Exporting…' : 'Export atlas'}
      </Button>
    {/if}
    <Button variant="secondary" size="sm" onclick={onReveal}>
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
      Reveal
    </Button>
    <Button variant="primary" size="sm" onclick={onClose}>Close</Button>
  {/snippet}
</Modal>

<style>
  .kind-badge {
    font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px;
    color: var(--accent); background: var(--accent-subtle);
    padding: 1px 7px; border-radius: 999px;
  }

  .preview-body { display: flex; flex-direction: column; gap: 16px; }

  .frame {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: var(--radius-md);
    overflow: hidden;
    color: #fff;
    box-shadow: inset 0 0 0 1px rgba(255,255,255,0.1);
  }
  /* Real media: letterbox on a dark backing so non-16:9 frames look intentional. */
  .frame.media { background: #05070b; }
  .frame.loading { display: flex; align-items: center; justify-content: center; }
  .seq-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-secondary); }
  .seq-desc { font-size: var(--font-size-xs); color: var(--text-muted); margin-top: 3px; max-width: 280px; }
  .media-el { display: block; width: 100%; height: 100%; object-fit: contain; }
  .frame-note {
    position: absolute; left: 10px; bottom: 10px;
    font-size: var(--font-size-2xs); font-weight: 500;
    color: #fff; background: rgba(0,0,0,0.42);
    padding: 3px 8px; border-radius: 6px;
  }
  .frame-dur {
    position: absolute; right: 10px; bottom: 10px;
    font-size: var(--font-size-sm); font-weight: 700; font-variant-numeric: tabular-nums;
    background: rgba(0,0,0,0.5); padding: 2px 8px; border-radius: 6px;
  }

  .meta { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 8px 20px; }
  .meta li {
    display: flex; align-items: center; gap: 8px;
    font-size: var(--font-size-sm); color: var(--text-secondary);
    padding: 7px 10px; background: var(--bg-input); border-radius: var(--radius-sm);
  }
  .meta li :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .meta li span { color: var(--text-muted); }
  .meta li b { margin-left: auto; color: var(--text-primary); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
