<script lang="ts">
  /**
   * Floating "find usages" popover (IntelliJ "Show Usages" style). Anchored at the
   * caret, driven by `usagesStore`. Keyboard-first: ↑/↓ move, Enter jumps + closes,
   * Esc closes; click a row to jump. Closes on a pointer-down outside. One mount in
   * the NemusShell.
   */
  import { SearchCode, CornerDownRight } from 'lucide-svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { usagesStore, type UsageItem } from '../stores/usages.svelte';

  const W = 380;
  const MAX_H = 320;
  const MARGIN = 8;

  // Position below-right of the caret, clamped to the viewport.
  const pos = $derived.by(() => {
    const a = usagesStore.anchor;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let x = a ? a.x : vw / 2 - W / 2;
    let y = a ? a.y + 6 : vh / 3;
    x = Math.min(Math.max(MARGIN, x), vw - W - MARGIN);
    y = Math.min(Math.max(MARGIN, y), vh - MAX_H - MARGIN);
    return { x, y };
  });

  let selected = $state(0);
  let panelEl = $state<HTMLElement | null>(null);

  // Fresh result set → reset selection + take focus so the arrows work at once.
  $effect(() => {
    if (!usagesStore.open) return;
    void usagesStore.symbol;
    void usagesStore.items;
    selected = 0;
    queueMicrotask(() => panelEl?.focus());
  });

  function jump(u: UsageItem) {
    nemusStore.requestGoto(u.offset, u.line);
    usagesStore.close();
  }

  function move(delta: number) {
    const n = usagesStore.count;
    if (!n) return;
    selected = (selected + delta + n) % n;
    panelEl?.querySelectorAll('.up-row')[selected]?.scrollIntoView({ block: 'nearest' });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); usagesStore.close(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') {
      e.preventDefault();
      const u = usagesStore.items[selected];
      if (u) jump(u);
    }
  }

  // Dismiss when the user clicks anywhere outside the popover.
  function onWindowPointerDown(e: PointerEvent) {
    if (!usagesStore.open) return;
    if (panelEl && e.target instanceof Node && panelEl.contains(e.target)) return;
    usagesStore.close();
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

{#if usagesStore.open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={panelEl}
    class="usage-pop"
    role="dialog"
    aria-label="Usages"
    tabindex="-1"
    style="left: {pos.x}px; top: {pos.y}px; width: {W}px; max-height: {MAX_H}px;"
    onkeydown={onKey}
  >
    <div class="up-head">
      <SearchCode size={12} />
      {#if usagesStore.symbol}
        <span class="up-title">Usages of <code>{usagesStore.symbol}</code></span>
        <span class="up-count">{usagesStore.count}</span>
      {:else}
        <span class="up-title">Find usages</span>
      {/if}
    </div>
    <div class="up-body">
      {#if usagesStore.symbol === null}
        <div class="up-empty">Place the caret on a name, then press <kbd>Alt</kbd>+<kbd>F7</kbd>.</div>
      {:else if usagesStore.count === 0}
        <div class="up-empty">No usages of <code>{usagesStore.symbol}</code> in this file.</div>
      {:else}
        {#each usagesStore.items as u, i (i)}
          <button
            class="up-row"
            class:sel={i === selected}
            onclick={() => jump(u)}
            onmousemove={() => (selected = i)}
          >
            <span class="up-icon"><CornerDownRight size={12} /></span>
            <span class="up-pos">{u.line}:{u.col}</span>
            <span class="up-preview">{u.preview}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .usage-pop {
    position: fixed;
    z-index: var(--z-popup, 1000);
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-popup);
    overflow: hidden;
    outline: none;
  }
  .up-head {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 9px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
  }
  .up-title { flex: 1; min-width: 0; font-size: 11.5px; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .up-title code { font-family: var(--font-code); color: var(--text-primary); }
  .up-count {
    font-size: 10px; font-weight: 700; font-variant-numeric: tabular-nums;
    padding: 0 5px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted);
  }

  .up-body { flex: 1; min-height: 0; overflow-y: auto; padding: 3px 0; }
  .up-empty { padding: 12px 14px; font-size: 11.5px; line-height: 1.5; color: var(--text-muted); }
  .up-empty code { font-family: var(--font-code); color: var(--text-secondary); }
  .up-empty kbd {
    font-family: var(--font-code); font-size: 10px; padding: 0 4px;
    background: var(--bg-overlay); border: 1px solid var(--border); border-radius: var(--radius-sm);
  }

  .up-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 4px 10px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
  }
  .up-row.sel { background: var(--accent-subtle); }
  .up-icon { display: flex; flex-shrink: 0; color: var(--text-disabled); }
  .up-pos {
    font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted);
    flex-shrink: 0; min-width: 38px; font-variant-numeric: tabular-nums;
  }
  .up-preview {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: 11.5px;
    color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .up-row.sel .up-preview { color: var(--text-primary); }
</style>
