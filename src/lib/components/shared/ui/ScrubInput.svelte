<script lang="ts">
  /**
   * ScrubInput — a number you can type, or drag. The small label on the left (an axis letter, a unit) is the
   * handle: press it and drag sideways to change the value (Shift = fine, Ctrl = coarse), double-click it to
   * reset. Typing works as in any field and commits on Enter / blur.
   *
   * `oninput` fires while dragging (throttled), `onchange` once when a drag or an edit ends — send the
   * second to something slow, the first to something that should follow your hand. App-agnostic.
   */
  let {
    value,
    label,
    step = 0.1,
    min,
    max,
    digits = 3,
    reset,
    unit,
    readonly = false,
    quiet = true,
    ariaLabel,
    oninput,
    onchange,
  }: {
    value: number;
    /** The drag handle: an axis letter, `°`, … Without one the field is typed only. */
    label?: string;
    /** Value change per pixel of drag at normal speed. */
    step?: number;
    min?: number;
    max?: number;
    /** Decimals shown. */
    digits?: number;
    /** Where a double-click on the handle puts the value back. */
    reset?: number;
    unit?: string;
    readonly?: boolean;
    quiet?: boolean;
    ariaLabel?: string;
    oninput?: (v: number) => void;
    onchange?: (v: number) => void;
  } = $props();

  const LIVE_EVERY_MS = 60;

  let dragging = $state(false);
  let draft = $state<string | null>(null);
  let startX = 0;
  let startValue = 0;
  let last = 0;
  let current = 0;

  const clamp = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v));
  const shown = $derived(draft ?? String(Number(value.toFixed(digits))));

  function down(e: PointerEvent) {
    if (readonly) return;
    dragging = true;
    startX = e.clientX;
    startValue = value;
    current = value;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!dragging) return;
    const speed = e.shiftKey ? 0.1 : e.ctrlKey ? 10 : 1;
    current = clamp(Number((startValue + (e.clientX - startX) * step * speed).toFixed(digits + 1)));
    const now = performance.now();
    if (now - last >= LIVE_EVERY_MS) {
      last = now;
      oninput?.(current);
    }
  }

  function up() {
    if (!dragging) return;
    dragging = false;
    if (current !== startValue) onchange?.(current);
  }

  function commit() {
    const text = draft;
    draft = null;
    if (text === null) return;
    const n = Number(text.replace(',', '.'));
    if (text.trim() !== '' && Number.isFinite(n) && clamp(n) !== value) onchange?.(clamp(n));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
    else if (e.key === 'Escape') draft = null;
    else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      const dir = e.key === 'ArrowUp' ? 1 : -1;
      const by = step * (e.shiftKey ? 0.1 : e.ctrlKey ? 10 : 1) * 10;
      onchange?.(clamp(Number((value + dir * by).toFixed(digits + 1))));
    }
  }
</script>

<span class="si" class:quiet class:dragging class:readonly>
  {#if label}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      class="si-handle"
      class:can={!readonly}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      ondblclick={() => reset !== undefined && !readonly && onchange?.(reset)}
      title={readonly ? undefined : reset === undefined ? 'Drag to change' : `Drag to change · double-click for ${reset}`}
    >{label}</span>
  {/if}
  <input
    class="si-input"
    style:width="{Math.max(3, (dragging ? String(Number(current.toFixed(digits))) : shown).length + 1)}ch"
    type="text"
    inputmode="decimal"
    value={dragging ? String(Number(current.toFixed(digits))) : shown}
    {readonly}
    aria-label={ariaLabel ?? label}
    oninput={(e) => (draft = (e.currentTarget as HTMLInputElement).value)}
    onblur={commit}
    onkeydown={onKey}
  />
  {#if unit}<span class="si-unit">{unit}</span>{/if}
</span>

<style>
  .si {
    display: inline-flex; align-items: center; gap: 0; min-width: 0; height: 22px;
    background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .si.quiet:not(:hover):not(:focus-within):not(.dragging) { background: transparent; border-color: transparent; }
  .si:focus-within, .si.dragging { border-color: var(--border-focus); }
  .si.readonly { background: transparent; }
  .si-handle {
    flex: none; padding: 0 5px; align-self: stretch; display: inline-flex; align-items: center;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    user-select: none; touch-action: none;
  }
  .si-handle.can { cursor: ew-resize; }
  .si-handle.can:hover, .si.dragging .si-handle { color: var(--accent); }
  .si-input {
    flex: 0 1 auto; min-width: 3ch; padding: 0 6px 0 0; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    font-variant-numeric: tabular-nums;
  }
  .si-unit { padding-right: 6px; font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
