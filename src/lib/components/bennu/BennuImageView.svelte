<script lang="ts">
  /**
   * An image, in an editor tab.
   *
   * Bennu used to refuse these: `.png` was in the binary list and clicking one in the project tree
   * said "binary file". That is the right answer for a `.jar` and the wrong one for the icon you
   * are about to reference from a `.ron` — an IDE that cannot show you an asset makes you leave it
   * to look at one.
   *
   * ## How the bytes get here
   *
   * Through Tauri's **asset protocol** (`convertFileSrc`), not through IPC. The alternative is
   * reading the file into base64 and handing the WebView a `data:` URL, which copies the whole
   * image twice — once as base64 across the seam, once as a decoded blob — and a 4 MB texture is a
   * 5.5 MB string in the middle of it. The asset URL lets the WebView stream the file itself, so
   * opening a large sprite sheet costs a decode and nothing else. It also means the browser's own
   * cache applies, which is what makes flipping between two tabs instant.
   *
   * Natural dimensions come from the `<img>` once it has decoded (`naturalWidth`), because that is
   * the only source that is right for every format without a parser per format. The byte size is a
   * separate, cheap `stat`.
   *
   * ## Zoom
   *
   * Two modes and no slider. **Fit** is the default because the first question about an asset is
   * "what is it"; **1:1** is the other one that matters, because the second question is "how big is
   * it really" and any intermediate percentage answers neither. Ctrl+scroll and the +/− keys step
   * through a fixed ladder for the case where you are looking at a 16×16 icon and need it bigger.
   *
   * The checkerboard is not decoration: half these files have an alpha channel, and on a flat dark
   * panel a transparent background is indistinguishable from a black one.
   */
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { Maximize2, Minus, Plus, Scan } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { fsPathsSize } from '$lib/ipc/fs';
  import { baseName } from '$lib/utils/paths';
  import { imageFormatLabel } from '$lib/utils/image-files';

  let { path }: { path: string } = $props();

  /** The asset URL for the file. Keyed on the path so switching tabs swaps the source. */
  const src = $derived(convertFileSrc(path));

  /** Zoom ladder, in percent. `null` means Fit. */
  const STEPS = [10, 25, 50, 75, 100, 150, 200, 400, 800, 1600];
  let zoom = $state<number | null>(null);

  let natural = $state<{ w: number; h: number } | null>(null);
  let bytes = $state<number | null>(null);
  let failed = $state(false);

  // A new file resets everything: the previous image's dimensions on a new one would be a lie for
  // as long as the decode takes, and that is exactly when they are read.
  $effect(() => {
    void path;
    natural = null;
    bytes = null;
    failed = false;
    zoom = null;
  });

  // The byte size, separately from the decode: `naturalWidth` says how big the picture is and
  // nothing about how big the file is, and for an asset both matter.
  $effect(() => {
    const p = path;
    void fsPathsSize([p])
      .then((s) => { if (p === path) bytes = s.bytes; })
      .catch(() => { /* a size we cannot read is a line we do not print */ });
  });

  function onLoad(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    natural = { w: img.naturalWidth, h: img.naturalHeight };
    failed = false;
  }

  /** Step the ladder. From Fit, the step starts at the nearest rung to 100%. */
  function step(delta: number) {
    const at = zoom === null ? STEPS.indexOf(100) : STEPS.indexOf(zoom);
    const from = at === -1 ? STEPS.indexOf(100) : at;
    zoom = STEPS[Math.max(0, Math.min(STEPS.length - 1, from + delta))];
  }

  /** Ctrl+scroll zooms, as it does everywhere else that shows an image. Plain scroll pans, which
   *  is the container's own job — so the handler is passive about anything without the modifier. */
  function onWheel(e: WheelEvent) {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    step(e.deltaY < 0 ? 1 : -1);
  }

  function onKey(e: KeyboardEvent) {
    switch (e.key) {
      case '+': case '=': e.preventDefault(); step(1); return;
      case '-': e.preventDefault(); step(-1); return;
      case '0': e.preventDefault(); zoom = 100; return;
      case 'f': case 'F': e.preventDefault(); zoom = null; return;
    }
  }

  function humanBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} kB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  const format = $derived(imageFormatLabel(path));
</script>

<div class="iv">
  <div class="iv-bar">
    <span class="iv-name">{baseName(path)}</span>
    <span class="iv-sep"></span>
    <button
      class="iv-btn"
      class:on={zoom === null}
      type="button"
      use:tooltip={{ content: 'Fit to the panel', shortcut: 'F' }}
      aria-label="Fit"
      aria-pressed={zoom === null}
      onclick={() => (zoom = null)}
    ><Scan size={13} /></button>
    <button
      class="iv-btn"
      class:on={zoom === 100}
      type="button"
      use:tooltip={{ content: 'Actual size', shortcut: '0' }}
      aria-label="Actual size"
      aria-pressed={zoom === 100}
      onclick={() => (zoom = 100)}
    ><Maximize2 size={13} /></button>
    <button class="iv-btn" type="button" use:tooltip={{ content: 'Zoom out', shortcut: '-' }}
      aria-label="Zoom out" onclick={() => step(-1)}><Minus size={13} /></button>
    <span class="iv-zoom">{zoom === null ? 'fit' : `${zoom}%`}</span>
    <button class="iv-btn" type="button" use:tooltip={{ content: 'Zoom in', shortcut: '+' }}
      aria-label="Zoom in" onclick={() => step(1)}><Plus size={13} /></button>
  </div>

  <!-- The scroller takes the keyboard so +/−/0/F work without a click on a button first. No role:
       what carries the accessible name is the `<img>` inside it (its `alt`), and a wrapper with a
       noninteractive role plus a tab stop is a contradiction. -->
  <!-- A scrollable region that has to take keys IS focusable — that is the WAI recommendation for
       one — and no interactive role describes it honestly. Claiming `button` or `slider` to satisfy
       the rule would be the worse trade: a screen reader would announce something this is not. -->
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
  <div class="iv-stage" tabindex="0" onwheel={onWheel} onkeydown={onKey}>
    {#if failed}
      <EmptyState message="This image could not be decoded." />
    {:else}
      <img
        class="iv-img"
        class:fit={zoom === null}
        {src}
        alt={baseName(path)}
        style={zoom === null ? undefined : `width: ${(natural?.w ?? 0) * (zoom / 100)}px`}
        onload={onLoad}
        onerror={() => { failed = true; }}
      />
    {/if}
  </div>

  <div class="iv-foot">
    <span>{format}</span>
    {#if natural}
      <span class="iv-dot">·</span><span>{natural.w} × {natural.h}</span>
    {/if}
    {#if bytes !== null}
      <span class="iv-dot">·</span><span>{humanBytes(bytes)}</span>
    {/if}
    <span class="iv-sep"></span>
    <span class="iv-hint">Ctrl+scroll to zoom</span>
  </div>
</div>

<style>
  .iv { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }

  .iv-bar {
    display: flex; align-items: center; gap: 4px; flex-shrink: 0;
    padding: 4px 8px; border-bottom: 1px solid var(--border-subtle);
  }
  .iv-name {
    font-size: var(--font-size-xs); color: var(--text-secondary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 40%;
  }
  .iv-sep { flex: 1; }
  .iv-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .iv-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
  .iv-btn.on { color: var(--accent); background: var(--accent-subtle); }
  .iv-zoom {
    min-width: 40px; text-align: center;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
  }

  .iv-stage {
    flex: 1; min-height: 0; overflow: auto; outline: none;
    display: flex; align-items: center; justify-content: center;
    padding: 12px;
    /* The checkerboard: a transparent PNG on a flat dark panel is indistinguishable from a black
       one, and which of the two it is changes what you do next. */
    background-color: var(--bg-overlay);
    background-image:
      linear-gradient(45deg, rgb(0 0 0 / 18%) 25%, transparent 25%, transparent 75%, rgb(0 0 0 / 18%) 75%),
      linear-gradient(45deg, rgb(0 0 0 / 18%) 25%, transparent 25%, transparent 75%, rgb(0 0 0 / 18%) 75%);
    background-size: 16px 16px;
    background-position: 0 0, 8px 8px;
  }
  .iv-stage:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  .iv-img {
    /* Nearest-neighbour above 1:1 so a 16×16 icon magnifies into pixels rather than into mush —
       which is the whole reason for zooming into one. */
    image-rendering: pixelated;
    flex-shrink: 0;
    max-width: none;
  }
  .iv-img.fit {
    max-width: 100%;
    max-height: 100%;
    width: auto;
    /* Fit never magnifies: a 16×16 icon blown up to fill the panel is a worse answer to "what is
       this" than a 16×16 icon. */
    image-rendering: auto;
    object-fit: contain;
  }

  .iv-foot {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    padding: 3px 10px; border-top: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }
  .iv-dot { color: var(--text-disabled); }
  .iv-hint { color: var(--text-disabled); }
</style>
