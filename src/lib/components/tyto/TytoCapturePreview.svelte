<script lang="ts">
  /**
   * TytoCapturePreview — a capture's detail / preview modal (mock). Shows a large
   * stand-in frame + full metadata + quick actions. The real frame/thumbnail will
   * come from the capture backend.
   */
  import { Video, Camera, FolderOpen, Trash2, Clock, HardDrive, Crosshair, CalendarClock } from 'lucide-svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import TytoThumb from './TytoThumb.svelte';
  import { formatDuration, formatBytes, type Capture } from '$lib/stores/tyto/recorder.svelte';

  let { capture, onClose, onReveal, onDelete }:
    { capture: Capture; onClose: () => void; onReveal: () => void; onDelete: () => void } = $props();

  const created = $derived(new Date(capture.createdAt).toLocaleString());
  // The on-disk file as an asset URL (empty in the mock → falls back to the stylized
  // stand-in). The asset-protocol scope already covers png/mp4 (tauri.conf.json).
  const mediaUrl = $derived(capture.path ? convertFileSrc(capture.path) : '');
</script>

<Modal {onClose} width="600px" ariaLabel="Capture preview">
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if capture.kind === 'record'}<Video size={15} />{:else}<Camera size={15} />{/if}
      <span class="modal-title">{capture.name}</span>
      <span class="kind-badge">{capture.kind === 'record' ? 'Recording' : 'Screenshot'}</span>
    </ModalHeader>
  {/snippet}

  <div class="preview-body">
    <div class="frame" class:media={!!mediaUrl}>
      {#if mediaUrl && capture.kind === 'record'}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video class="media-el" src={mediaUrl} controls autoplay preload="auto"></video>
      {:else if mediaUrl}
        <img class="media-el" src={mediaUrl} alt={capture.name} />
      {:else}
        <TytoThumb hue={capture.hue} kind={capture.kind} />
        {#if capture.kind === 'record'}
          <span class="frame-dur">{formatDuration(capture.durationMs ?? 0)}</span>
        {/if}
        <span class="frame-note">Stylized preview · real frame comes with the backend</span>
      {/if}
    </div>

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
