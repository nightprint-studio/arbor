<script lang="ts">
  /**
   * Full-screen image preview overlay. Singleton — mounted once in AppShell and
   * driven by `imageLightbox`. Inline images in issue/MR/PR bodies (enhanced by
   * the `previewImages` action) open it; it pages across the sibling images in
   * the body they came from.
   *
   * Keyboard: Esc closes, ←/→ page between images, Enter/click toggles zoom.
   */
  import { X, ChevronLeft, ChevronRight, ZoomIn, ZoomOut } from 'lucide-svelte';
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { imageLightbox } from '$lib/stores/imageLightbox.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let zoomed = $state(false);

  const current  = $derived(imageLightbox.current);
  const hasMany  = $derived(imageLightbox.items.length > 1);

  // Reset zoom whenever the shown image changes (paging or reopening).
  let lastSrc = $state<string | null>(null);
  $effect(() => {
    if (current?.src !== lastSrc) {
      lastSrc = current?.src ?? null;
      zoomed = false;
    }
  });

  function close() { imageLightbox.close(); }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) close();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!imageLightbox.open) return;
    if (e.key === 'Escape')          { e.preventDefault(); close(); }
    else if (e.key === 'ArrowRight') { e.preventDefault(); imageLightbox.next(); }
    else if (e.key === 'ArrowLeft')  { e.preventDefault(); imageLightbox.prev(); }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if imageLightbox.open && current}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="lb-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Image preview"
    tabindex="-1"
    onclick={onBackdropClick}
    transition:fade={{ duration: animStore.dBase }}
  >
    <div class="lb-toolbar">
      {#if hasMany}
        <span class="lb-counter">{imageLightbox.index + 1} / {imageLightbox.items.length}</span>
      {/if}
      <button
        class="lb-btn"
        type="button"
        onclick={() => (zoomed = !zoomed)}
        use:tooltip={zoomed ? 'Fit to screen' : 'Zoom in'}
        aria-label={zoomed ? 'Fit to screen' : 'Zoom in'}
      >
        {#if zoomed}<ZoomOut size={16} />{:else}<ZoomIn size={16} />{/if}
      </button>
      <button class="lb-btn" type="button" onclick={close} use:tooltip={'Close (Esc)'} aria-label="Close">
        <X size={16} />
      </button>
    </div>

    {#if hasMany}
      <button class="lb-nav lb-prev" type="button" onclick={imageLightbox.prev} aria-label="Previous image">
        <ChevronLeft size={28} />
      </button>
      <button class="lb-nav lb-next" type="button" onclick={imageLightbox.next} aria-label="Next image">
        <ChevronRight size={28} />
      </button>
    {/if}

    <div class="lb-stage" class:zoomed onclick={onBackdropClick}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <img
        src={current.src}
        alt={current.alt}
        class="lb-img"
        class:zoomed
        onclick={(e) => { e.stopPropagation(); zoomed = !zoomed; }}
        transition:scale={{ start: 0.94, duration: animStore.dPanel, easing: cubicOut }}
      />
    </div>

    {#if current.alt}
      <div class="lb-caption">{current.alt}</div>
    {/if}
  </div>
{/if}

<style>
  .lb-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-top);
    background: rgba(0, 0, 0, 0.88);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .lb-toolbar {
    position: absolute;
    top: 12px;
    right: 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    z-index: 2;
  }
  .lb-counter {
    font-size: 12px;
    color: var(--text-secondary);
    background: rgba(0, 0, 0, 0.4);
    padding: 4px 10px;
    border-radius: var(--radius-md);
    margin-right: 2px;
  }
  .lb-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    color: var(--text-primary);
    cursor: pointer;
    transition: background var(--anim-dur-base, 120ms) ease, border-color var(--anim-dur-base, 120ms) ease;
  }
  .lb-btn:hover { background: var(--bg-hover); border-color: var(--border); }
  .lb-btn:focus-visible { outline: 2px solid var(--border-focus); outline-offset: 1px; }

  .lb-nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 64px;
    border: none;
    border-radius: var(--radius-lg);
    background: rgba(0, 0, 0, 0.35);
    color: var(--text-primary);
    cursor: pointer;
    z-index: 2;
    transition: background var(--anim-dur-base, 120ms) ease;
  }
  .lb-nav:hover { background: rgba(0, 0, 0, 0.6); }
  .lb-nav:focus-visible { outline: 2px solid var(--border-focus); outline-offset: 1px; }
  .lb-prev { left: 14px; }
  .lb-next { right: 14px; }

  .lb-stage {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    padding: 56px 64px;
  }
  .lb-stage.zoomed {
    overflow: auto;
    align-items: flex-start;
    justify-content: flex-start;
    padding: 56px;
  }

  .lb-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    cursor: zoom-in;
    user-select: none;
  }
  .lb-img.zoomed {
    max-width: none;
    max-height: none;
    cursor: zoom-out;
  }

  .lb-caption {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    max-width: 80vw;
    text-align: center;
    font-size: 12px;
    color: var(--text-secondary);
    background: rgba(0, 0, 0, 0.45);
    padding: 6px 12px;
    border-radius: var(--radius-md);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
