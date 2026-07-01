<!--
  ConflictResultPanel — preview/edit pane below the diff columns.

  Renders the *computed* merge result (or the user's manual override),
  pre-rendered via the syntax highlighter, with an invisible textarea over
  the top so the user can edit. Editing flips the file into "manual mode";
  a reset button reverts to the selection-derived result.

  Owns:
    · the resize handle (drag the top edge to set a pixel height)
    · the collapse button + animated open/close
    · scroll sync between the highlighted <pre> and the editable <textarea>

  Does NOT own the height state — the parent does, because the same resize
  height should persist across file switches within one modal session.
-->
<script lang="ts">
  import { ChevronUp, ChevronDown } from 'lucide-svelte';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Pre-highlighted HTML for the result text. */
    highlightedHtml: string;
    /** Raw text value — keeps the textarea in sync with selection changes. */
    value:           string;
    /** True when the user has typed into the textarea (overrides selection). */
    isManual:        boolean;
    collapsed:       boolean;
    /** Pixel height when the user has dragged the handle. `null` = 50/50 flex split. */
    height:          number | null;

    onCollapseToggle: () => void;
    onHeightChange:   (next: number | null) => void;
    onInput:          (value: string) => void;
    onReset:          () => void;
  }

  let {
    highlightedHtml, value, isManual,
    collapsed, height,
    onCollapseToggle, onHeightChange, onInput, onReset,
  }: Props = $props();

  let preEl       = $state<HTMLPreElement | null>(null);
  let textareaEl  = $state<HTMLTextAreaElement | null>(null);
  let _resizeStart: { y: number; h: number } | null = null;

  // Resize: capture the actual rendered height at drag start so subsequent
  // movements compute absolute pixel sizes (not a delta) — that way the
  // handle never jumps even after the flex layout has been resolved.
  function startResize(e: MouseEvent) {
    const el = (e.currentTarget as HTMLElement).parentElement;
    const h = el ? el.getBoundingClientRect().height : 200;
    _resizeStart = { y: e.clientY, h };
    e.preventDefault();
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }
  function onMove(e: MouseEvent) {
    if (!_resizeStart) return;
    const delta = _resizeStart.y - e.clientY;
    onHeightChange(Math.max(80, Math.min(560, _resizeStart.h + delta)));
  }
  function onUp() {
    _resizeStart = null;
    window.removeEventListener('mousemove', onMove);
    window.removeEventListener('mouseup', onUp);
  }

  function syncScroll() {
    if (textareaEl && preEl) {
      preEl.scrollTop  = textareaEl.scrollTop;
      preEl.scrollLeft = textareaEl.scrollLeft;
    }
  }

  // Collapse animation: drive the height (and the parent's flex-grow) on
  // every frame from JS. CSS transitions on `flex-grow` are unreliable
  // across browser versions — going through `tick` guarantees motion.
  function resultSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const h = node.getBoundingClientRect().height;
    const parent = node.parentElement;
    return {
      duration,
      easing: cubicOut,
      tick: (t: number) => {
        node.style.height = `${t * h}px`;
        node.style.minHeight = '0';
        node.style.overflow = 'hidden';
        if (parent) {
          parent.style.flexGrow = String(t);
          parent.style.minHeight = `${t * 80}px`;
        }
      },
    };
  }
</script>

<div
  class="blocking-result"
  class:is-collapsed={collapsed}
  style={!collapsed && height !== null ? `flex: 0 0 ${height}px` : ''}
>
  <div
    class="result-resize-handle"
    onmousedown={startResize}
    role="separator"
    aria-orientation="horizontal"
    tabindex="-1"
    aria-hidden="true"
    use:tooltip={'Drag to resize'}
  ></div>
  <div class="result-header">
    <button
      class="result-collapse-btn"
      onclick={onCollapseToggle}
      use:tooltip={collapsed ? 'Expand result panel' : 'Collapse result panel'}
      aria-label={collapsed ? 'Expand result' : 'Collapse result'}
    >
      {#if collapsed}
        <ChevronUp size={12} />
      {:else}
        <ChevronDown size={12} />
      {/if}
    </button>
    <span class="result-header-title">Merge result</span>
    <span class="result-header-hint">
      {#if isManual}
        <span class="result-manual-badge">manually edited</span>
      {:else}
        based on selection
      {/if}
    </span>
    {#if isManual && !collapsed}
      <button class="result-reset-btn" onclick={onReset} use:tooltip={'Reset to selection'}>
        ↩ Reset
      </button>
    {/if}
    <div class="result-header-spacer"></div>
  </div>
  {#if !collapsed}
    <div class="result-editor-wrap" transition:resultSlide={{ duration: animStore.dPanel }}>
      <pre class="result-highlight" bind:this={preEl} aria-hidden="true">{@html highlightedHtml}</pre>
      <textarea
        class="result-textarea"
        bind:this={textareaEl}
        {value}
        oninput={(e) => { onInput(e.currentTarget.value); syncScroll(); }}
        onscroll={syncScroll}
        spellcheck="false"
      ></textarea>
    </div>
  {/if}
</div>

<style>
  .blocking-result {
    flex-grow: 1;
    flex-shrink: 1;
    flex-basis: 0;
    min-height: 80px;
    border-top: 1px solid var(--border-subtle);
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    position: relative;
    overflow: hidden;
  }
  .blocking-result.is-collapsed .result-resize-handle { display: none; }

  .result-resize-handle {
    position: absolute; top: -4px; left: 0; right: 0; height: 8px;
    cursor: ns-resize; z-index: 10;
    display: flex; align-items: center; justify-content: center;
  }
  .result-resize-handle::after {
    content: '';
    width: 40px; height: 3px;
    border-radius: 2px;
    background: var(--border);
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .result-resize-handle:hover::after { opacity: 1; }

  .result-header {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    cursor: default;
  }
  .result-collapse-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px;
    padding: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 50%;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .result-collapse-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }

  .result-header-title {
    font-size: 11px; font-weight: 600; color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .result-header-hint {
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
  }
  .result-manual-badge {
    font-size: 10px;
    background: rgba(77,120,204,.15);
    color: var(--accent);
    border: 1px solid rgba(77,120,204,.3);
    border-radius: 999px;
    padding: 0 6px;
    font-family: var(--font-ui-sans);
  }
  .result-reset-btn {
    margin-left: auto;
    background: none; border: none; cursor: pointer;
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
    padding: 2px 6px; border-radius: var(--radius-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .result-reset-btn:hover { color: var(--text-secondary); background: var(--bg-hover); }
  .result-header-spacer { flex: 1; }

  .result-editor-wrap {
    flex: 1; position: relative; overflow: hidden; min-height: 0;
  }

  .result-highlight {
    position: absolute; inset: 0;
    margin: 0; padding: 8px 12px;
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    color: var(--text-primary); background: var(--bg-base);
    white-space: pre; overflow: hidden;
    pointer-events: none; user-select: none;
    border: none;
    tab-size: 2;
  }

  .result-textarea {
    position: absolute; inset: 0;
    resize: none; outline: none;
    background: transparent; border: none;
    color: transparent; caret-color: var(--text-primary);
    font-family: var(--font-code); font-size: 12px; line-height: 1.6;
    padding: 8px 12px; min-height: 0;
    overflow: auto; z-index: 1;
    tab-size: 2;
  }
  .result-textarea::selection {
    background: color-mix(in srgb, var(--accent) calc(35% * var(--selection-strength, 1)), transparent);
  }
</style>
