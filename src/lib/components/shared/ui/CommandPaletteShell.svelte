<script module lang="ts">
  /** A single selectable row. Domain data (branches, commands, …) is mapped to
   *  this shape by the host before handing sections to the shell. */
  export interface PaletteItem {
    id:         string;
    title:      string;
    subtitle?:  string;
    /** Icon key resolved through the host's `iconResolver`. */
    icon:       string;
    /** CSS colour applied to the icon + title (e.g. branch lane palette). */
    iconColor?: string;
    /** Shortcut hint rendered on the right (split on `+` into key caps). */
    shortcut?:  string;
    /** Render the title in the monospace face — for identifiers (branch/tag). */
    mono?:      boolean;
    /** Show the "opens a target picker" chevron + run-on-Tab semantics. */
    isVerb?:    boolean;
    action:     () => void | Promise<void>;
  }

  export interface PaletteSection {
    id:    string;
    label: string;
    items: PaletteItem[];
  }

  /** Phase-2 indicator: the chip rendered left of the input once a verb is picked. */
  export interface PaletteVerbChip {
    title: string;
    icon:  string;
  }
</script>

<script lang="ts">
  /**
   * CommandPaletteShell — the agnostic two-phase command-palette engine shared
   * by the main window and the merula window.
   *
   * It owns the overlay, the input row (search icon + optional verb chip + ghost
   * autocomplete + close button), the sectioned listbox, the footer hints, and
   * the whole keyboard model (↑/↓ move, Enter run, Esc close, Tab complete/enter
   * verb, Backspace clear verb). It knows nothing about git, merula, stores or
   * IPC: the host builds `sections` (already filtered + scored for the current
   * query/phase) and the shell renders + drives them.
   *
   * Phase 1 (no `verbChip`): `sections` are commands/verbs. Picking a verb item
   * is just running its `action`, which the host implements by setting a
   * `verbChip` and re-deriving `sections` into the target list (phase 2).
   *
   * Host extension points:
   *  - `iconResolver(name)`  → maps an icon key to a Svelte component.
   *  - `onClearVerb`         → back out of phase 2 (Backspace at col 0 / chip click).
   *  - `onKeydownCapture(e)` → intercept host-specific keys (e.g. multi-step
   *                            verbs) BEFORE the shell's own handling; return
   *                            true to consume the event.
   *  - `emptyMessage` snippet → domain-specific "nothing here" copy.
   */
  import type { Snippet } from 'svelte';
  import type { IconComponent } from '$lib/types/icon';
  import { tick } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Command, ChevronRight } from 'lucide-svelte';
  import Spinner from './Spinner.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { isMac } from '$lib/utils/platform';
  import { macKeyLabel } from '$lib/utils/keybindings';

  interface Props {
    onClose: () => void;
    /** Resolve an icon key to a component. Unknown keys should fall back. */
    iconResolver: (name: string) => IconComponent;
    /** Host-built sections, already filtered + scored for the current query. */
    sections: PaletteSection[];
    /** The live query — bound so the host can react with its own `$effect`. */
    query?: string;
    /** Bound so the host can manage focus/caret on phase transitions. */
    inputEl?: HTMLInputElement | null;
    placeholder?: string;
    /** Phase-2 chip; null/undefined → phase 1. */
    verbChip?: PaletteVerbChip | null;
    onClearVerb?: () => void;
    /** Shortcut hint shown on the chip tooltip + footer (default `Backspace`). */
    clearVerbShortcut?: string;
    /** Centre a spinner instead of the empty message while targets load. */
    loading?: boolean;
    loadingLabel?: string;
    /** Copy rendered when there are no items (and not loading). */
    emptyMessage?: Snippet;
    /** Phase-1-only trailing footer hint (e.g. "or type verb + space"). */
    phase1Hint?: string;
    /** Intercept keydown before the shell. Return true to consume. */
    onKeydownCapture?: (e: KeyboardEvent) => boolean;
    width?: string;
  }

  let {
    onClose,
    iconResolver,
    sections,
    query = $bindable(''),
    inputEl = $bindable(null),
    placeholder = 'Type a command…',
    verbChip = null,
    onClearVerb,
    clearVerbShortcut = 'Backspace',
    loading = false,
    loadingLabel = 'Loading…',
    emptyMessage,
    phase1Hint,
    onKeydownCapture,
    width = 'min(640px, 90vw)',
  }: Props = $props();

  let selectedIdx = $state(0);
  let listEl = $state<HTMLElement | null>(null);

  const flatItems = $derived(sections.flatMap((s) => s.items));

  // Ghost autocomplete: the tail of the first item whose title starts with the
  // query. Drives the inline grey preview + Tab-to-complete.
  const ghostSuffix = $derived.by(() => {
    if (!query || flatItems.length === 0) return '';
    const lq = query.toLowerCase();
    const first = flatItems.find((i) => i.title.toLowerCase().startsWith(lq));
    return first ? first.title.slice(query.length) : '';
  });

  // Reset the cursor to the top whenever the query or phase changes; the host's
  // section rebuild reorders everything, so a stale index would point nowhere.
  let lastReset = '';
  // (touch: cache-bust after repairing a stray null byte in this template string)
  $effect(() => {
    const key = `${query}${verbChip?.title ?? ''}`;
    if (key !== lastReset) {
      lastReset = key;
      selectedIdx = 0;
    }
  });

  // Keep the selection in range as the list shrinks.
  $effect(() => {
    if (selectedIdx >= flatItems.length) selectedIdx = Math.max(0, flatItems.length - 1);
  });

  $effect(() => { inputEl?.focus(); });

  function escHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  /** Wrap the matched run of `q` in `title` with `<span class="hl">`. */
  function highlightTitle(title: string, q: string): string {
    if (!q) return escHtml(title);
    const lq = q.toLowerCase();
    const lt = title.toLowerCase();
    if (lt.startsWith(lq)) {
      return `<span class="cps-hl">${escHtml(title.slice(0, q.length))}</span>${escHtml(title.slice(q.length))}`;
    }
    const idx = lt.indexOf(lq);
    if (idx !== -1) {
      return escHtml(title.slice(0, idx))
        + `<span class="cps-hl">${escHtml(title.slice(idx, idx + q.length))}</span>`
        + escHtml(title.slice(idx + q.length));
    }
    return escHtml(title);
  }

  function scrollIntoView() {
    tick().then(() => {
      listEl?.querySelector<HTMLElement>(`[data-idx="${selectedIdx}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    });
  }

  function onKeydown(e: KeyboardEvent) {
    // Host-specific keys win first (e.g. multi-step verbs).
    if (onKeydownCapture?.(e)) return;

    // Backspace at column 0 backs out of phase 2.
    if (e.key === 'Backspace' && verbChip && query === '') {
      e.preventDefault();
      onClearVerb?.();
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, flatItems.length - 1);
      scrollIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      scrollIntoView();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      flatItems[selectedIdx]?.action();
    } else if (e.key === 'Tab' && ghostSuffix) {
      e.preventDefault();
      // For a verb item, run its action (enter the chip) rather than filling
      // the title — otherwise the host's auto-promote would re-parse the words.
      const lq = query.toLowerCase();
      const matched = flatItems.find((i) => i.title.toLowerCase().startsWith(lq));
      if (matched?.isVerb) {
        matched.action();
      } else {
        query = query + ghostSuffix;
        tick().then(() => {
          if (inputEl) inputEl.selectionStart = inputEl.selectionEnd = query.length;
        });
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  /**
   * The caps to draw for an item's chord.
   *
   * Folded to macOS glyphs there, the same way `formatBinding` does for the keybinding settings:
   * a palette that prints `Ctrl` on a Mac is telling the user to press a key that, for most of
   * Arbor's chords, is spelled ⌘ — and for the ones on the Space bar is not merely spelled
   * differently but genuinely does not work.
   */
  function shortcutKeys(s: string): string[] {
    return (isMac ? macKeyLabel(s) : s).split('+').map((k) => k.trim()).filter(Boolean);
  }
</script>

<!-- Backdrop -->
<div
  class="cps-backdrop"
  role="presentation"
  onmousedown={(e) => { if (e.target === e.currentTarget) onClose(); }}
  transition:fade={{ duration: animStore.dBase }}
>
  <div
    class="cps-container"
    role="dialog"
    aria-modal="true"
    aria-label="Command Palette"
    style:width={width}
    transition:fly={{ y: -16, duration: animStore.dPanel, easing: cubicOut }}
  >
    <!-- Input row -->
    <div class="cps-header">
      <Command size={15} class="cps-search" />

      {#if verbChip}
        {@const VerbIcon = iconResolver(verbChip.icon)}
        <button
          class="cps-chip"
          onclick={onClearVerb}
          use:tooltip={{ content: 'Clear verb', shortcut: clearVerbShortcut }}
          aria-label="Clear {verbChip.title} verb"
        >
          <VerbIcon size={12} />
          <span class="cps-chip-label">{verbChip.title}</span>
          <ChevronRight size={12} class="cps-chip-arrow" />
        </button>
      {/if}

      <div class="cps-input-wrap">
        <input
          bind:this={inputEl}
          bind:value={query}
          onkeydown={onKeydown}
          {placeholder}
          autocomplete="off"
          spellcheck="false"
          class="cps-input"
        />
        {#if ghostSuffix}
          <span class="cps-ghost" aria-hidden="true">
            <span class="cps-ghost-typed">{query}</span><span class="cps-ghost-suffix">{ghostSuffix}</span>
          </span>
        {/if}
      </div>

      {#if ghostSuffix}
        <kbd class="cps-tab-hint">Tab</kbd>
      {/if}
      <button
        class="close-btn"
        onclick={onClose}
        use:tooltip={{ content: 'Close', shortcut: 'Esc' }}
        aria-label="Close"
      ></button>
    </div>

    <!-- Results -->
    <div class="cps-results" bind:this={listEl}>
      {#if flatItems.length === 0 && loading}
        <div class="cps-loading"><Spinner size="md" label={loadingLabel} /></div>
      {:else if flatItems.length === 0}
        <div class="cps-empty">
          {#if emptyMessage}{@render emptyMessage()}{:else}No results{/if}
        </div>
      {:else}
        {#each sections as section (section.id)}
          <div class="cps-section">{section.label}</div>
          {#each section.items as item (item.id)}
            {@const idx = flatItems.indexOf(item)}
            {@const isSelected = idx === selectedIdx}
            {@const ItemIcon = iconResolver(item.icon)}
            <button
              class="cps-item"
              class:selected={isSelected}
              class:mono={item.mono}
              data-idx={idx}
              onmouseenter={() => { selectedIdx = idx; }}
              onclick={() => item.action()}
              use:tooltip={item.subtitle ?? item.title}
            >
              <span class="cps-item-icon" style={item.iconColor ? `color: ${item.iconColor}` : ''}>
                <ItemIcon size={14} />
              </span>
              <span class="cps-item-body">
                <span class="cps-item-title" style={item.iconColor ? `color: ${item.iconColor}` : ''}>
                  {@html highlightTitle(item.title, query)}
                </span>
                {#if item.subtitle}
                  <span class="cps-item-sub">{item.subtitle}</span>
                {/if}
              </span>
              {#if item.shortcut}
                <span class="cps-keys">
                  {#each shortcutKeys(item.shortcut) as key, i}
                    {#if i > 0}<span class="cps-key-plus" aria-hidden="true">+</span>{/if}
                    <kbd class="cps-key">{key}</kbd>
                  {/each}
                </span>
              {/if}
              {#if item.isVerb}
                <span class="cps-verb-marker" use:tooltip={'Select to pick a target'}>
                  <ChevronRight size={14} />
                </span>
              {/if}
              {#if isSelected}<span class="cps-enter">↵</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    <!-- Footer -->
    <div class="cps-footer">
      <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
      <span><kbd>↵</kbd> {verbChip ? 'run' : 'pick command'}</span>
      {#if ghostSuffix}<span><kbd>Tab</kbd> complete</span>{/if}
      {#if verbChip}
        <span><kbd>⌫</kbd> clear verb</span>
      {:else if phase1Hint}
        <span class="cps-hint-muted">{phase1Hint}</span>
      {/if}
      <span><kbd>Esc</kbd> close</span>
    </div>
  </div>
</div>

<style>
  /* ── Backdrop ─────────────────────────────────────────────────────────── */
  .cps-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-menu);
    /* No `backdrop-filter: blur()` (WebView2 compositor stall) — dim instead. */
    background: rgba(0, 0, 0, 0.78);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
  }

  /* ── Container ────────────────────────────────────────────────────────── */
  .cps-container {
    max-height: 70vh;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 32px 80px rgba(0, 0, 0, 0.7), 0 0 0 1px rgba(255, 255, 255, 0.04);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Header ───────────────────────────────────────────────────────────── */
  .cps-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  :global(.cps-search) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  /* ── Verb chip (phase-2 indicator) ────────────────────────────────────── */
  .cps-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px 3px 9px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 999px;
    color: var(--accent);
    font: 600 12px/1 var(--font-ui-sans);
    letter-spacing: 0.01em;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--anim-dur-fast), border-color var(--anim-dur-fast);
    animation: cpsChipIn var(--anim-dur-fast) ease-out;
  }
  .cps-chip:hover {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .cps-chip-label { white-space: nowrap; }
  :global(.cps-chip-arrow) { opacity: 0.7; }

  @keyframes cpsChipIn {
    from { opacity: 0; transform: translateX(-6px); }
    to   { opacity: 1; transform: none; }
  }

  /* ── Input + ghost ────────────────────────────────────────────────────── */
  .cps-input-wrap {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
  }
  .cps-input {
    width: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font: 13px/1.4 var(--font-ui-sans);
    caret-color: var(--accent);
    position: relative;
    z-index: 1;
  }
  .cps-input::placeholder { color: var(--text-disabled); }

  .cps-ghost {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    font: 13px/1.4 var(--font-ui-sans);
    pointer-events: none;
    white-space: pre;
    z-index: 0;
  }
  .cps-ghost-typed  { color: transparent; }
  .cps-ghost-suffix { color: var(--text-disabled); }

  .cps-tab-hint {
    font-size: var(--font-size-2xs);
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: var(--bg-base);
    flex-shrink: 0;
  }

  /* ── Results ──────────────────────────────────────────────────────────── */
  .cps-results {
    flex: 1;
    overflow-y: auto;
    padding: 6px 0 4px;
    scroll-behavior: smooth;
  }

  .cps-empty {
    padding: 32px 20px;
    text-align: center;
    color: var(--text-muted);
    font-size: var(--font-size-md);
  }
  .cps-loading {
    padding: 32px 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: var(--font-size-md);
  }

  .cps-section {
    padding: 8px 16px 3px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-disabled);
    user-select: none;
  }

  .cps-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100% - 12px);
    margin: 1px 6px;
    padding: 6px 10px;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-size: var(--font-size-md);
    font-family: var(--font-ui-sans);
    transition: background 80ms;
    border-radius: var(--radius-sm);
  }
  .cps-item:hover { background: var(--bg-hover); }
  .cps-item.selected {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    outline: none;
  }

  .cps-item-icon {
    color: var(--text-muted);
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }
  .cps-item.selected .cps-item-icon { color: var(--accent); }

  .cps-item-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .cps-item-title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Identifiers (branch/tag) read better monospaced next to prose commands. */
  .cps-item.mono .cps-item-title {
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
  }
  :global(.cps-item-title .cps-hl) {
    color: var(--accent);
    font-weight: 600;
  }
  .cps-item-sub {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Keycap chips mirror the shared Kbd widget's muted "box" variant — the shell
     can't import Kbd (shared/ui → shared/internal is disallowed), so the look is
     replicated inline: split on `+`, one chip per chord part. */
  .cps-keys {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .cps-key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    min-width: 18px;
    height: 17px;
    padding: 0 5px;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border-subtle, var(--border));
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }
  .cps-key-plus {
    color: var(--text-muted);
    font-size: var(--font-size-2xs);
    user-select: none;
  }

  .cps-enter {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    flex-shrink: 0;
    opacity: 0.7;
  }

  .cps-verb-marker {
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    flex-shrink: 0;
    transition: color var(--anim-dur-fast), transform var(--anim-dur-fast);
  }
  .cps-item.selected .cps-verb-marker {
    color: var(--accent);
    transform: translateX(2px);
  }

  /* ── Footer ───────────────────────────────────────────────────────────── */
  .cps-footer {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 7px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-elevated);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    flex-shrink: 0;
  }
  .cps-footer kbd {
    font-size: var(--font-size-2xs);
    padding: 1px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-muted);
  }
  .cps-footer span { display: flex; align-items: center; gap: 4px; }
</style>
