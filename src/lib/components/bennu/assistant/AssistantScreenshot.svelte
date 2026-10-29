<script lang="ts">
  /**
   * A screenshot the assistant took of a page on this machine, drawn as a thumbnail under the tool's receipt.
   * The thread carries only a path; the picture is fetched when this appears, and opens larger in a modal.
   */
  import { Image as ImageIcon, Maximize2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { getAssistantImage } from '$lib/ipc/bennu/assistant';
  import type { ScreenshotShot } from './assistant-types';

  let { shot }: { shot: ScreenshotShot } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  let open = $state(false);

  $effect(() => {
    const path = shot.path;
    src = null;
    failed = false;
    getAssistantImage(path)
      .then((img) => { if (path === shot.path) src = `data:${img.media_type};base64,${img.data}`; })
      .catch(() => { failed = true; });
  });
</script>

<div class="shot">
  {#if src}
    <button type="button" class="thumb" onclick={() => (open = true)} aria-label="Enlarge the screenshot">
      <img {src} alt={shot.caption} />
      <span class="zoom"><Maximize2 size={12} /></span>
    </button>
  {:else if failed}
    <div class="gone"><ImageIcon size={13} /> The screenshot is no longer on disk.</div>
  {:else}
    <div class="gone"><Spinner size={12} /></div>
  {/if}
  <span class="caption">{shot.caption}</span>
</div>

{#if open && src}
  <Modal onClose={() => (open = false)} width="1100px" height="760px" padBody={false} bodyBorder ariaLabel="Screenshot">
    {#snippet header()}
      <ModalHeader onClose={() => (open = false)}>
        <ImageIcon size={14} />
        <span class="title">{shot.caption}</span>
      </ModalHeader>
    {/snippet}
    <div class="full"><img {src} alt={shot.caption} /></div>
  </Modal>
{/if}

<style>
  .shot { display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .thumb {
    position: relative; display: block; padding: 0; cursor: zoom-in;
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    background: var(--bg-elevated); overflow: hidden; max-width: 100%;
  }
  .thumb img { display: block; max-width: 100%; max-height: 220px; }
  .zoom {
    position: absolute; top: 6px; right: 6px; display: flex; padding: 3px; border-radius: 5px;
    background: var(--bg-overlay); color: var(--text-secondary); opacity: 0; transition: opacity var(--transition-fast);
  }
  .thumb:hover .zoom, .thumb:focus-visible .zoom { opacity: 1; }
  .gone { display: flex; align-items: center; gap: 6px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .caption { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .title { font-size: var(--font-size-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .full { height: 100%; overflow: auto; display: flex; align-items: flex-start; justify-content: center; background: var(--bg-base); }
  .full img { max-width: 100%; height: auto; }
</style>
