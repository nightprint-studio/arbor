<script lang="ts">
  /**
   * VirtualTextView — generic, line-virtualised text viewport.
   *
   * App-agnostic: renders a scrollable list of lines where only the visible
   * window (± overscan) is ever in the DOM, so a multi-MB file / live log stays
   * fluid. Every row is a fixed-height (`lineHeight`) box, absolutely offset via
   * a `translateY` window over a full-height spacer.
   *
   * Rendering per line is fully caller-controlled via the `line` snippet (pass
   * highlighted / ANSI-tokenised markup); when omitted, rows show the raw text
   * (escaped by Svelte). An optional built-in line-number gutter is available
   * via `showLineNumbers`; callers that want a custom gutter (e.g. highlighted
   * code) render it inside their own `line` snippet instead.
   *
   * Tailing: `follow` (bindable) auto-scrolls to the bottom as lines land, and
   * detaches when the user scrolls up past the bottom threshold, re-arming when
   * they scroll back down. `scrollToBottom()` is exported for a Follow button.
   *
   * Keyboard: the viewport is focusable (role=log by default) and handles
   * PageUp / PageDown / Home / End for scroll-only navigation.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no IPC/stores, no imports from
   * shared/internal — pure generic props + a render snippet.
   */
  import type { Snippet } from 'svelte';

  interface Props {
    /** Flat list of lines to render (one row each). */
    lines: string[];
    /** Fixed row height in px (must match the snippet's rendered height). */
    lineHeight?: number;
    /** Extra rows rendered above/below the viewport to smooth fast scrolls. */
    overscan?: number;
    /** Show a built-in right-aligned line-number gutter for the raw fallback.
     *  Ignored when a `line` snippet is supplied (render your own gutter there). */
    showLineNumbers?: boolean;
    /** Tail-follow: auto-scroll to the bottom as new lines arrive. Two-way
     *  bindable — set false when the user scrolls up, true when they scroll back
     *  to the bottom, so a caller "Follow" toggle stays in sync. */
    follow?: boolean;
    /** Extra class on the scroll viewport. */
    class?: string;
    /** ARIA role for the viewport (default 'log' — a streaming region). */
    role?: string;
    /** Per-row wrapper class (e.g. 'log-line line-stderr'). Applied in addition
     *  to the widget's own row class so consumer `:global(.log-line …)` rules
     *  keep matching. */
    rowClass?: (text: string, index: number) => string | undefined;
    /** Shown centred when `lines` is empty and `waiting` is false. */
    emptyMessage?: string;
    /** Show the waiting indicator instead of the empty message. */
    waiting?: boolean;
    /** Text shown alongside the waiting indicator. */
    waitingMessage?: string;
    /** Stable per-row keys for `{#each}` reconciliation. Length must match
     *  `lines`; falls back to the absolute row index when omitted. */
    keys?: ReadonlyArray<string | number>;
    /** Custom per-line renderer. Receives the row text + absolute index. When
     *  omitted the raw (escaped) text is shown. */
    line?: Snippet<[{ text: string; index: number }]>;
    /** Optional snippet for the empty state (overrides `emptyMessage`). */
    empty?: Snippet;
  }

  let {
    lines,
    lineHeight      = 19,
    overscan        = 20,
    showLineNumbers = false,
    follow          = $bindable(false),
    class: klass    = '',
    role            = 'log',
    rowClass,
    emptyMessage    = 'No content.',
    waiting         = false,
    waitingMessage  = 'Waiting…',
    keys,
    line,
    empty,
  }: Props = $props();

  let scrollEl  = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH     = $state(0);
  let raf       = 0;

  const totalH   = $derived(lines.length * lineHeight);
  const start    = $derived(Math.max(0, Math.floor(scrollTop / lineHeight) - overscan));
  const end      = $derived(Math.min(
    lines.length,
    Math.ceil((scrollTop + Math.max(viewH, lineHeight)) / lineHeight) + overscan,
  ));
  const windowed = $derived(lines.slice(start, end));
  const offset   = $derived(start * lineHeight);
  const gutterW  = $derived(String(lines.length).length);

  const BOTTOM_THRESHOLD = 40;
  function atBottom(): boolean {
    if (!scrollEl) return true;
    return scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < BOTTOM_THRESHOLD;
  }

  function onScroll() {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      if (!scrollEl) return;
      scrollTop = scrollEl.scrollTop;
      // Manual scroll away from the bottom pauses follow; scrolling back re-arms.
      follow = atBottom();
    });
  }

  // Re-pin to the bottom whenever the line count changes while following.
  $effect(() => {
    // Reactive dep: recompute on every push.
    const _ = lines.length;
    if (follow && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
      scrollTop = scrollEl.scrollTop;
    }
  });

  /** Scroll to the bottom and re-arm follow. Exported for a Follow / jump-to-end
   *  button. */
  export function scrollToBottom() {
    if (!scrollEl) return;
    scrollEl.scrollTop = scrollEl.scrollHeight;
    scrollTop = scrollEl.scrollTop;
    follow = true;
  }
  /** Scroll to a given line index (top-aligned). */
  export function scrollToIndex(i: number) {
    if (!scrollEl) return;
    scrollEl.scrollTop = Math.max(0, i) * lineHeight;
    scrollTop = scrollEl.scrollTop;
    follow = atBottom();
  }

  function onKeydown(e: KeyboardEvent) {
    if (!scrollEl) return;
    const page = Math.max(lineHeight, scrollEl.clientHeight - lineHeight);
    switch (e.key) {
      case 'PageDown': e.preventDefault(); scrollEl.scrollTop += page; break;
      case 'PageUp':   e.preventDefault(); scrollEl.scrollTop -= page; break;
      case 'Home':     e.preventDefault(); scrollEl.scrollTop = 0; break;
      case 'End':      e.preventDefault(); scrollEl.scrollTop = scrollEl.scrollHeight; break;
      default: return;
    }
    scrollTop = scrollEl.scrollTop;
    follow = atBottom();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="vtv {klass}"
  class:empty={lines.length === 0}
  bind:this={scrollEl}
  bind:clientHeight={viewH}
  onscroll={onScroll}
  onkeydown={onKeydown}
  {role}
  tabindex="0"
>
  {#if lines.length === 0}
    <div class="vtv-empty">
      {#if empty}
        {@render empty()}
      {:else if waiting}
        <span class="vtv-waiting">{waitingMessage}</span>
      {:else}
        <span>{emptyMessage}</span>
      {/if}
    </div>
  {:else}
    <div class="vtv-spacer" style="height: {totalH}px;">
      <div class="vtv-window" style="transform: translateY({offset}px);">
        {#each windowed as text, i (keys && keys.length === lines.length ? keys[start + i] : start + i)}
          {@const idx = start + i}
          <div
            class="vtv-row {rowClass ? (rowClass(text, idx) ?? '') : ''}"
            style="height: {lineHeight}px; line-height: {lineHeight}px;"
          >
            {#if line}
              {@render line({ text, index: idx })}
            {:else}
              {#if showLineNumbers}
                <span class="vtv-ln" style="width: {gutterW}ch;">{idx + 1}</span>
              {/if}
              <span class="vtv-text">{text}</span>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .vtv {
    flex: 1;
    min-height: 0;
    overflow: auto;
    position: relative;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }
  /* Centre the empty / waiting message. */
  .vtv.empty { display: flex; align-items: center; justify-content: center; }

  .vtv-spacer { position: relative; width: 100%; }
  .vtv-window { position: absolute; top: 0; left: 0; width: 100%; }

  .vtv-row { display: flex; align-items: flex-start; white-space: pre; box-sizing: border-box; }

  .vtv-ln {
    flex-shrink: 0;
    text-align: right;
    padding: 0 10px 0 14px;
    color: var(--text-muted);
    opacity: 0.55;
    user-select: none;
    -webkit-user-select: none;
  }
  .vtv-text { min-width: 0; }

  .vtv-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  .vtv-waiting {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    justify-content: center;
  }
</style>
