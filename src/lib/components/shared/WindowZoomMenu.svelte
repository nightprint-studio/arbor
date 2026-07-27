<script lang="ts">
  /**
   * WindowZoomMenu — the "Move & Resize" panel macOS pops when you hover the
   * green zoom button. Arbor paints its own because our windows are frameless
   * on every platform, so the OS never renders one for us.
   *
   * Laid out like the real thing: one titled section per category (fill/center,
   * halves, quarters), a display switcher when more than one monitor is
   * attached, and "Return to Previous Size" at the foot. Every tile is drawn
   * from `ZONE_FRACTIONS` — preview glyph and actual snap read the same table,
   * so adding a zone is one entry in `utils/window-tiling`.
   *
   * Keyboard-first: arrows walk the rows (each section keeps its own column
   * count), Enter applies, Escape closes, and the caption line names whatever
   * the pointer or the focus ring is on.
   */
  import { onMount, tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { CornerUpLeft, Monitor } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { displaysStore } from '$lib/stores/displays.svelte';
  import {
    applyZone, restorePrevious, moveToDisplay, hasPreviousGeometry, zoneRect,
    ZONE_GROUPS, ZONE_LABELS, type TileZone,
  } from '$lib/utils/window-tiling';

  interface Props {
    /** Live rect of the button the menu hangs from (viewport coords). */
    anchor: DOMRect;
    /** Close request — the owner clears its flag and restores focus. */
    onClose: () => void;
    /** Pointer entered the panel (cancels the owner's hover-close timer). */
    onHoverIn?: () => void;
    /** Pointer left the panel (arms the owner's hover-close timer). */
    onHoverOut?: () => void;
  }

  let { anchor, onClose, onHoverIn, onHoverOut }: Props = $props();

  type Entry =
    | { kind: 'zone';    zone: TileZone;  label: string; caption: string; disabled?: boolean }
    | { kind: 'display'; index: number;   label: string; caption: string; disabled?: boolean }
    | { kind: 'restore';                  label: string; caption: string; disabled?: boolean };

  type Section = { title: string; cols: number; entries: Entry[] };

  const PANEL_W = 304;

  // Captured once — the panel is re-created on every open, so a plain read is
  // exactly "was there a previous geometry when this panel opened?".
  const canRestore = hasPreviousGeometry();

  const displays = $derived(displaysStore.list);

  const sections = $derived.by(() => {
    const out: Section[] = ZONE_GROUPS.map(g => ({
      title: g.title,
      cols:  g.cols,
      entries: g.zones.map((zone): Entry => ({
        kind: 'zone', zone, label: ZONE_LABELS[zone], caption: ZONE_LABELS[zone],
      })),
    }));
    if (displays.length > 1) {
      out.push({
        title: 'Displays',
        cols: Math.min(displays.length, 3),
        entries: displays.map((d): Entry => ({
          kind: 'display', index: d.index, label: d.label,
          caption: d.name ? `${d.label} — ${d.name} · ${d.width}×${d.height}`
                          : `${d.label} — ${d.width}×${d.height}`,
          disabled: d.current,
        })),
      });
    }
    if (canRestore) {
      out.push({
        title: '',
        cols: 1,
        entries: [{ kind: 'restore', label: 'Return to Previous Size', caption: 'Return to Previous Size' }],
      });
    }
    return out;
  });

  /** Flat row model for keyboard nav: every section chunked by its own `cols`. */
  const rows = $derived.by(() => {
    const out: { section: number; entries: Entry[] }[] = [];
    sections.forEach((s, si) => {
      for (let i = 0; i < s.entries.length; i += s.cols) {
        out.push({ section: si, entries: s.entries.slice(i, i + s.cols) });
      }
    });
    return out;
  });

  let panelEl = $state<HTMLElement | undefined>();
  let row     = $state(0);
  let col     = $state(0);

  const focused = $derived(rows[row]?.entries[col]);
  const caption = $derived(focused?.caption ?? '');

  /** Fixed placement under the button, clamped to the viewport. */
  const style = $derived.by(() => {
    const GAP = 8, MARGIN = 6;
    const left = Math.max(
      MARGIN,
      Math.min(anchor.left - 10, window.innerWidth - PANEL_W - MARGIN),
    );
    return `left:${Math.round(left)}px;top:${Math.round(anchor.bottom + GAP)}px;width:${PANEL_W}px;`;
  });

  onMount(() => {
    // Monitors come and go with docks and cables — re-read on every open.
    void displaysStore.refresh();
    void tick().then(() => panelEl?.focus());
  });

  function isFocused(entry: Entry): boolean {
    return focused === entry;
  }

  async function activate(entry: Entry | undefined) {
    if (!entry || entry.disabled) return;
    onClose();
    if      (entry.kind === 'zone')    await applyZone(entry.zone);
    else if (entry.kind === 'display') await moveToDisplay(entry.index);
    else                               await restorePrevious();
  }

  function focusAt(r: number, c: number) {
    const total = rows.length;
    if (!total) return;
    row = ((r % total) + total) % total;
    col = Math.max(0, Math.min(c, rows[row].entries.length - 1));
  }

  function step(delta: 1 | -1) {
    const line = rows[row];
    if (!line) return;
    const next = col + delta;
    if (next >= 0 && next < line.entries.length) { col = next; return; }
    // Off the end of a row — carry over to the neighbouring one.
    const r = ((row + delta) % rows.length + rows.length) % rows.length;
    row = r;
    col = delta > 0 ? 0 : rows[r].entries.length - 1;
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case 'Escape': e.preventDefault(); e.stopPropagation(); onClose(); return;
      case 'Tab':        onClose(); return;
      case 'ArrowRight': e.preventDefault(); step(1);  return;
      case 'ArrowLeft':  e.preventDefault(); step(-1); return;
      case 'ArrowDown':  e.preventDefault(); focusAt(row + 1, col); return;
      case 'ArrowUp':    e.preventDefault(); focusAt(row - 1, col); return;
      case 'Home':       e.preventDefault(); focusAt(0, 0); return;
      case 'End':        e.preventDefault(); focusAt(rows.length - 1, 99); return;
      case 'Enter':
      case ' ': e.preventDefault(); void activate(focused); return;
    }
  }

  /** Point the focus at an entry the pointer is over. */
  function focusEntry(entry: Entry) {
    for (let r = 0; r < rows.length; r += 1) {
      const c = rows[r].entries.indexOf(entry);
      if (c >= 0) { row = r; col = c; return; }
    }
  }

  // Outside click closes, same contract as Dropdown's menus.
  $effect(() => {
    function onOut(e: PointerEvent) {
      const t = e.target as Node;
      if (!panelEl?.contains(t)) onClose();
    }
    document.addEventListener('pointerdown', onOut, { capture: true });
    return () => document.removeEventListener('pointerdown', onOut, { capture: true } as EventListenerOptions);
  });

  /** Preview box the zone glyphs are drawn in (viewBox units). */
  const GLYPH = { x: 0, y: 0, width: 36, height: 24 };
</script>

<!-- The panel's transition is `|global`: it is mounted by the OWNER's `{#if}`,
     so a local transition would never play (nested creation). -->
<!-- svelte-ignore a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
<div
  class="wz-panel"
  bind:this={panelEl}
  {style}
  role="menu"
  tabindex="-1"
  aria-label="Move and resize window"
  onkeydown={onKeydown}
  onpointerenter={() => onHoverIn?.()}
  onpointerleave={() => onHoverOut?.()}
  transition:fly|global={{ y: -4, duration: animStore.dFast, easing: cubicOut }}
>
  {#each sections as section (section.title || 'restore')}
    {#if section.title}
      <div class="wz-title">{section.title}</div>
    {:else}
      <div class="wz-sep" aria-hidden="true"></div>
    {/if}

    <!-- Roomy rows (2 columns or fewer) carry the label beside the glyph; the
         dense 3-4 column grids are icon-only, with the caption naming them. -->
    {@const labelled = section.cols <= 2}
    <div class="wz-grid" style:--wz-cols={section.cols}>
      {#each section.entries as entry (entry.kind + entry.label)}
        <button
          type="button"
          class="wz-tile"
          class:wz-wide={labelled}
          class:focused={isFocused(entry)}
          disabled={entry.disabled}
          role="menuitem"
          aria-label={entry.label}
          onpointerenter={() => focusEntry(entry)}
          onclick={() => void activate(entry)}
        >
          {#if entry.kind === 'zone'}
            {@const r = zoneRect(entry.zone, GLYPH)}
            <svg viewBox="-1.5 -1.5 39 27" width="44" height="29" aria-hidden="true">
              <rect
                x="0" y="0" width={GLYPH.width} height={GLYPH.height} rx="3.5"
                fill="none" stroke="currentColor" stroke-width="1.4" opacity="0.35"
              />
              <rect
                x={r.x + 1.2} y={r.y + 1.2}
                width={Math.max(r.width - 2.4, 1)} height={Math.max(r.height - 2.4, 1)}
                rx="2" fill="currentColor"
              />
            </svg>
            {#if labelled}<span class="wz-tile-label">{entry.label}</span>{/if}
          {:else if entry.kind === 'display'}
            <Monitor size={15} />
            <span class="wz-tile-label">{entry.label}</span>
          {:else}
            <CornerUpLeft size={14} />
            <span class="wz-tile-label">{entry.label}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/each}

  <div class="wz-caption">{caption}</div>
</div>

<style>
  .wz-panel {
    position: fixed;
    z-index: var(--z-menu);
    display: flex;
    flex-direction: column;
    padding: 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    font-family: var(--font-ui-sans);
    outline: none;
    -webkit-app-region: no-drag;
  }

  .wz-title {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 8px 2px 5px;
  }
  /* First section sits flush with the panel's own padding. */
  .wz-title:first-child { padding-top: 0; }

  .wz-sep {
    height: 1px;
    background: var(--border-subtle);
    margin: 9px 2px 6px;
  }

  .wz-grid {
    display: grid;
    grid-template-columns: repeat(var(--wz-cols, 4), 1fr);
    gap: 5px;
  }

  /* The preview glyph is drawn in `currentColor`, so the tile's text colour is
     also the "window" it draws: primary text keeps it bright against the dark
     tile instead of the muddy grey a secondary tone gives. */
  .wz-tile {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 42px;
    padding: 0 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast),
                color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .wz-tile.wz-wide { justify-content: flex-start; }
  /* No separate :hover rule — pointing at a tile makes it the focused one, so
     hover and keyboard focus share a single highlight. */
  .wz-tile.focused:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }
  .wz-tile:disabled { opacity: 0.4; cursor: default; }

  .wz-tile-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wz-caption {
    padding: 9px 2px 0;
    font-size: 11px;
    color: var(--text-secondary);
    text-align: center;
    min-height: 18px;
  }
</style>
