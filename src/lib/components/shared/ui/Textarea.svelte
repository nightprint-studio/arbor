<script lang="ts">
  /**
   * A multi-line text field that grows with what is typed, up to a ceiling, then scrolls — the shape of
   * a chat or comment box, where one line is the common case and twelve is the exception.
   *
   * Bare on purpose: no border or background of its own, so it can sit inside a composer that draws
   * one frame around the field and its controls. `Input` is the framed single-line field.
   *
   * `Ctrl/Cmd+Enter` calls `onsubmit`; plain Enter is a newline, as in every multi-line field.
   */
  let {
    value = $bindable(''),
    placeholder = '',
    minRows = 1,
    maxRows = 8,
    disabled = false,
    ariaLabel,
    onsubmit,
    element = $bindable<HTMLTextAreaElement | undefined>(),
  }: {
    value?: string;
    placeholder?: string;
    minRows?: number;
    /** Where growing stops and scrolling starts. */
    maxRows?: number;
    disabled?: boolean;
    ariaLabel?: string;
    onsubmit?: () => void;
    element?: HTMLTextAreaElement;
  } = $props();

  // Sized from the content's own height, after collapsing it to find out what that is — the only way
  // a textarea reports it. The ceiling is rows of the field's own line height, not a pixel guess.
  function fit() {
    const el = element;
    if (!el) return;
    el.style.height = 'auto';
    const line = parseFloat(getComputedStyle(el).lineHeight) || 20;
    const pad = el.offsetHeight - el.clientHeight;
    const max = line * maxRows + pad;
    el.style.height = `${Math.min(el.scrollHeight + pad, max)}px`;
    el.style.overflowY = el.scrollHeight + pad > max ? 'auto' : 'hidden';
  }

  $effect(() => {
    void value;
    fit();
  });

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      onsubmit?.();
    }
  }
</script>

<textarea
  bind:this={element}
  bind:value
  class="ta"
  rows={minRows}
  {placeholder}
  {disabled}
  aria-label={ariaLabel ?? placeholder}
  onkeydown={onKeydown}
  oninput={fit}
></textarea>

<style>
  .ta {
    display: block;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    padding: 0;
  }
  .ta::placeholder { color: var(--text-muted); }
  .ta:disabled { opacity: 0.5; }
</style>
