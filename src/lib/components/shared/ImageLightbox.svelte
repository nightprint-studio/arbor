<script lang="ts">
  /**
   * Full-screen image preview overlay. Singleton — mounted once in AppShell and
   * driven by `imageLightbox`. Inline images in issue/MR/PR bodies (enhanced by
   * the `previewImages` action) and the pictures in Imber's thread open it; it
   * pages across the sibling images they came from.
   *
   * Zoom is a factor over the *fitted* size (1 = the whole picture on screen), so
   * zooming always enlarges — even a picture smaller than the window. The point
   * under the cursor stays where it is while zooming; a zoomed picture is panned
   * by dragging or scrolling.
   *
   * Keyboard: Esc closes, ←/→ page between images, + / − zoom, 0 fits,
   * Enter/click toggles 2×. Ctrl/⌘ + wheel (or a pinch) zooms at the cursor.
   */
  import { tick } from 'svelte';
  import { X, ChevronLeft, ChevronRight, ZoomIn, ZoomOut, Minimize2 } from 'lucide-svelte';
  import { fade, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { imageLightbox } from '$lib/stores/imageLightbox.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const MAX_ZOOM = 8;
  const STEP = 1.25;
  /** The stage's padding, kept clear of the toolbar and the caption. */
  const PAD_X = 64;
  const PAD_Y = 56;
  /** A press that moves less than this is a click, not a drag. */
  const DRAG_SLOP = 4;

  let zoom = $state(1);
  let natural = $state<{ w: number; h: number } | null>(null);
  let stageW = $state(0);
  let stageH = $state(0);
  let stage = $state<HTMLDivElement | null>(null);
  let img = $state<HTMLImageElement | null>(null);

  const current  = $derived(imageLightbox.current);
  const hasMany  = $derived(imageLightbox.items.length > 1);
  const zoomed   = $derived(zoom > 1);

  /** The scale that fits the whole picture in the stage — never above its own size. */
  const fit = $derived(
    natural && stageW && stageH
      ? Math.min(1, (stageW - 2 * PAD_X) / natural.w, (stageH - 2 * PAD_Y) / natural.h)
      : 1,
  );
  const size = $derived(natural ? { w: natural.w * fit * zoom, h: natural.h * fit * zoom } : null);

  // Reset zoom whenever the shown image changes (paging or reopening).
  let lastSrc = $state<string | null>(null);
  $effect(() => {
    if (current?.src !== lastSrc) {
      lastSrc = current?.src ?? null;
      zoom = 1;
      natural = null;
    }
  });

  function onLoad() {
    if (img) natural = { w: img.naturalWidth, h: img.naturalHeight };
  }

  /** Zoom to `next`, keeping the picture point under (`x`, `y`) — the stage's centre by default — in place. */
  async function zoomTo(next: number, x?: number, y?: number) {
    next = Math.min(MAX_ZOOM, Math.max(1, next));
    if (!stage || !img) { zoom = next; return; }
    const box = stage.getBoundingClientRect();
    const px = x ?? box.left + box.width / 2;
    const py = y ?? box.top + box.height / 2;
    const before = img.getBoundingClientRect();
    const fx = Math.min(1, Math.max(0, (px - before.left) / before.width));
    const fy = Math.min(1, Math.max(0, (py - before.top) / before.height));
    zoom = next;
    await tick();
    const after = img.getBoundingClientRect();
    stage.scrollLeft += after.left + fx * after.width - px;
    stage.scrollTop += after.top + fy * after.height - py;
  }

  function close() { imageLightbox.close(); }

  // ── Pan by dragging ───────────────────────────────────────────────────────
  let drag: { x: number; y: number; left: number; top: number } | null = null;
  let dragged = false;

  function onPointerDown(e: PointerEvent) {
    dragged = false;
    if (!stage || e.button !== 0 || !zoomed) return;
    drag = { x: e.clientX, y: e.clientY, left: stage.scrollLeft, top: stage.scrollTop };
  }
  function onPointerMove(e: PointerEvent) {
    if (!drag || !stage) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!dragged && Math.hypot(dx, dy) < DRAG_SLOP) return;
    if (!dragged) stage.setPointerCapture(e.pointerId);
    dragged = true;
    stage.scrollLeft = drag.left - dx;
    stage.scrollTop = drag.top - dy;
  }
  function onPointerUp() { drag = null; }

  function onWheel(e: WheelEvent) {
    // A trackpad pinch arrives as a wheel with ctrlKey; plain scrolling pans.
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    void zoomTo(zoom * Math.pow(STEP, -e.deltaY / 100), e.clientX, e.clientY);
  }

  // Attached by hand, not with `onwheel`: zooming must cancel the page zoom / scroll, and a listener Svelte may
  // register as passive cannot `preventDefault`.
  $effect(() => {
    const el = stage;
    if (!el) return;
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  function onStageClick(e: MouseEvent) {
    if (!dragged && e.target === e.currentTarget) close();
  }

  function onImageClick(e: MouseEvent) {
    e.stopPropagation();
    if (dragged) return;
    // `detail === 0` is Enter/Space on the button: no cursor, zoom at the centre.
    const at = e.detail === 0 ? [] : [e.clientX, e.clientY];
    void zoomTo(zoomed ? 1 : 2, ...at);
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) close();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!imageLightbox.open) return;
    if (e.key === 'Escape')          { e.preventDefault(); close(); }
    else if (e.key === 'ArrowRight') { e.preventDefault(); imageLightbox.next(); }
    else if (e.key === 'ArrowLeft')  { e.preventDefault(); imageLightbox.prev(); }
    else if (e.key === '+' || e.key === '=') { e.preventDefault(); void zoomTo(zoom * STEP); }
    else if (e.key === '-' || e.key === '_') { e.preventDefault(); void zoomTo(zoom / STEP); }
    else if (e.key === '0')          { e.preventDefault(); void zoomTo(1); }
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
      <button class="lb-btn" type="button" onclick={() => zoomTo(zoom / STEP)} disabled={!zoomed} use:tooltip={'Zoom out (−)'} aria-label="Zoom out">
        <ZoomOut size={16} />
      </button>
      <span class="lb-counter lb-percent">{Math.round(fit * zoom * 100)}%</span>
      <button class="lb-btn" type="button" onclick={() => zoomTo(zoom * STEP)} disabled={zoom >= MAX_ZOOM} use:tooltip={'Zoom in (+)'} aria-label="Zoom in">
        <ZoomIn size={16} />
      </button>
      <button class="lb-btn" type="button" onclick={() => zoomTo(1)} disabled={!zoomed} use:tooltip={'Fit to screen (0)'} aria-label="Fit to screen">
        <Minimize2 size={16} />
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

    <div
      class="lb-stage"
      class:zoomed
      class:dragging={drag !== null}
      bind:this={stage}
      bind:clientWidth={stageW}
      bind:clientHeight={stageH}
      onclick={onStageClick}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
    >
      <!-- Button wrapper (display:contents — transparent to the flex layout) so the
           zoom toggle is a real interactive element; an <img> can't carry an
           interactive role. -->
      <button
        type="button"
        class="lb-img-btn"
        aria-label={zoomed ? 'Fit to screen' : 'Zoom in'}
        onclick={onImageClick}
      >
        <img
          bind:this={img}
          src={current.src}
          alt={current.alt}
          class="lb-img"
          class:sized={size !== null}
          class:zoomed
          style:width={size ? `${size.w}px` : null}
          style:height={size ? `${size.h}px` : null}
          draggable="false"
          onload={onLoad}
          transition:scale={{ start: 0.94, duration: animStore.dPanel, easing: cubicOut }}
        />
      </button>
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
    font-size: var(--font-size-sm);
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

  /* The picture centres with `margin: auto`, not with justify/align: a centred flex item larger than its box
     overflows on both sides and its left and top halves cannot be scrolled to. */
  .lb-stage {
    display: flex;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    padding: 56px 64px;
    overflow: auto;
  }
  .lb-stage.zoomed { cursor: grab; }
  .lb-stage.dragging { cursor: grabbing; }

  /* Transparent to the flex layout: the <img> centers in .lb-stage exactly as
     before; the button only adds the interactive semantics + keyboard handling. */
  .lb-img-btn {
    display: contents;
  }
  .lb-img {
    flex: none;
    margin: auto;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    cursor: zoom-in;
    user-select: none;
  }
  /* Once its size is known it is set exactly (fitted × zoom); the max limits only hold before that. */
  .lb-img.sized {
    max-width: none;
    max-height: none;
  }
  .lb-img.zoomed { cursor: inherit; }
  .lb-percent { min-width: 52px; text-align: center; font-variant-numeric: tabular-nums; }
  .lb-btn:disabled { opacity: 0.4; cursor: default; }

  .lb-caption {
    position: absolute;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    max-width: 80vw;
    text-align: center;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    background: rgba(0, 0, 0, 0.45);
    padding: 6px 12px;
    border-radius: var(--radius-md);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
