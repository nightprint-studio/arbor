<script lang="ts" generics="T">
  /**
   * Floating keyboard-driven picker — the shared chrome behind nemus's editor
   * popovers (find-usages, file-structure). Owns the one thing every floating list
   * needs and nothing domain-specific: viewport-clamped positioning (anchored at
   * the caret, or centred when there's no anchor), an optional filter field, arrow
   * navigation (↑/↓ wrap, Enter selects, Esc closes), dismiss-on-outside-click, and
   * the selected-row scroll-into-view. The consumer supplies the header, the per-
   * row markup, and the (already-filtered) items via snippets — so one popover look
   * lives in one place and a new picker is just a list + a row snippet.
   *
   * Generic over the item type `T`; `bind:filterText` drives the consumer's own
   * filtering (the picker stays oblivious to how an item matches a query).
   */
  import type { Snippet } from 'svelte';

  let {
    open,
    anchor = null,
    width = 380,
    maxHeight = 320,
    items,
    onSelect,
    onClose,
    filterable = false,
    placeholder = 'Filter…',
    filterText = $bindable(''),
    ariaLabel = 'Picker',
    header,
    row,
    empty,
  }: {
    open: boolean;
    /** Caret/viewport anchor; `null` centres the panel near the top. */
    anchor?: { x: number; y: number } | null;
    width?: number;
    maxHeight?: number;
    items: T[];
    onSelect: (item: T, index: number) => void;
    onClose: () => void;
    filterable?: boolean;
    placeholder?: string;
    filterText?: string;
    ariaLabel?: string;
    header?: Snippet;
    /** Row body for one item; receives the item and whether it is selected. */
    row: Snippet<[T, boolean]>;
    /** Shown in place of the list when `items` is empty. */
    empty?: Snippet;
  } = $props();

  const MARGIN = 8;

  // Below-right of the caret when anchored, else centred-near-top — both clamped
  // so the panel never spills past the viewport.
  const pos = $derived.by(() => {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let x = anchor ? anchor.x : vw / 2 - width / 2;
    let y = anchor ? anchor.y + 6 : Math.max(MARGIN, vh / 6);
    x = Math.min(Math.max(MARGIN, x), vw - width - MARGIN);
    y = Math.min(Math.max(MARGIN, y), vh - maxHeight - MARGIN);
    return { x, y };
  });

  let selected = $state(0);
  let panelEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);
  // Who held focus (the editor) before we stole it — restored when the picker is
  // cancelled with Esc so the caret doesn't vanish. Captured only on the open
  // transition (not on every filter keystroke, which would capture our own input).
  let prevFocus: HTMLElement | null = null;
  let wasOpen = false;

  // Fresh open OR a changed result set (e.g. live filtering) → reset to the top and
  // take focus so the keyboard works immediately (the filter field if present,
  // else the panel for arrow-nav).
  $effect(() => {
    if (open && !wasOpen) prevFocus = document.activeElement as HTMLElement | null;
    wasOpen = open;
    if (!open) return;
    void items;
    selected = 0;
    queueMicrotask(() => (filterable ? inputEl : panelEl)?.focus());
  });

  /** Hand focus back to whoever had it (the editor) — keeps the caret visible. */
  function restoreFocus() { prevFocus?.focus?.(); }

  function move(delta: number) {
    const n = items.length;
    if (!n) return;
    selected = (selected + delta + n) % n;
    panelEl?.querySelectorAll('.fp-row')[selected]?.scrollIntoView({ block: 'nearest' });
  }

  function choose(i: number) {
    const it = items[i];
    if (it !== undefined) onSelect(it, i);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); restoreFocus(); onClose(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === 'Enter') { e.preventDefault(); choose(selected); }
  }

  // Dismiss when the user clicks anywhere outside the panel.
  function onWindowPointerDown(e: PointerEvent) {
    if (!open) return;
    if (panelEl && e.target instanceof Node && panelEl.contains(e.target)) return;
    onClose();
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={panelEl}
    class="fp"
    role="dialog"
    aria-label={ariaLabel}
    tabindex="-1"
    style="left: {pos.x}px; top: {pos.y}px; width: {width}px; max-height: {maxHeight}px;"
    onkeydown={onKey}
  >
    {#if header}
      <div class="fp-head">{@render header()}</div>
    {/if}
    {#if filterable}
      <div class="fp-filter">
        <input
          bind:this={inputEl}
          bind:value={filterText}
          {placeholder}
          spellcheck="false"
          autocapitalize="off"
          autocomplete="off"
        />
      </div>
    {/if}
    <div class="fp-body">
      {#if items.length === 0}
        {#if empty}{@render empty()}{:else}<div class="fp-empty">No results.</div>{/if}
      {:else}
        {#each items as it, i (i)}
          <button
            class="fp-row"
            class:sel={i === selected}
            onclick={() => choose(i)}
            onmousemove={() => (selected = i)}
          >
            {@render row(it, i === selected)}
          </button>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .fp {
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

  .fp-head {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 9px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
  }

  .fp-filter {
    flex-shrink: 0;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fp-filter input {
    width: 100%;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: 12px;
    outline: none;
  }
  .fp-filter input:focus { border-color: var(--border-focus, var(--accent)); }
  .fp-filter input::placeholder { color: var(--text-disabled); }

  .fp-body { flex: 1; min-height: 0; overflow-y: auto; padding: 3px 0; }

  .fp-row {
    display: flex; align-items: center; gap: 8px;
    width: 100%; text-align: left;
    padding: 4px 10px; cursor: pointer;
    background: transparent; border: none; font-family: var(--font-ui-sans);
  }
  .fp-row.sel { background: var(--accent-subtle); }
</style>
