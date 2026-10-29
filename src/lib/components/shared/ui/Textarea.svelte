<script lang="ts">
  /**
   * A multi-line text field that grows with what is typed, up to a ceiling, then scrolls — the shape of
   * a chat or comment box, where one line is the common case and twelve is the exception.
   *
   * Bare by default: no border or background of its own, so it can sit inside a composer that draws
   * one frame around the field and its controls. `framed` draws that frame itself (border, background,
   * a focus ring) for a field that stands alone in a form; `maxLength` caps the text and shows a
   * `n / max` counter under it. `Input` is the framed single-line field.
   *
   * `Ctrl/Cmd+Enter` calls `onsubmit`; plain Enter is a newline, as in every multi-line field.
   */
  let {
    value = $bindable(''),
    placeholder = '',
    minRows = 1,
    maxRows = 8,
    disabled = false,
    framed = false,
    maxLength,
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
    /** Draw a border and background around the field (and the counter). */
    framed?: boolean;
    /** Most characters the field accepts; a counter shows how many are used. */
    maxLength?: number;
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

  const count = $derived(value.length);
  const nearLimit = $derived(maxLength !== undefined && count >= maxLength * 0.9);
</script>

{#snippet field()}
  <textarea
    bind:this={element}
    bind:value
    class="ta"
    rows={minRows}
    {placeholder}
    {disabled}
    maxlength={maxLength}
    aria-label={ariaLabel ?? placeholder}
    onkeydown={onKeydown}
    oninput={fit}
  ></textarea>
  {#if maxLength !== undefined}
    <span class="count" class:near={nearLimit} aria-hidden="true">{count} / {maxLength}</span>
  {/if}
{/snippet}

{#if framed}
  <div class="frame" class:disabled>
    {@render field()}
  </div>
{:else}
  {@render field()}
{/if}

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

  .frame {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-base);
    padding: 6px 8px;
  }
  .frame:focus-within { border-color: var(--accent); }
  .frame.disabled { opacity: 0.6; }

  .count {
    display: block;
    margin-top: 4px;
    text-align: right;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .count.near { color: var(--warning); }
</style>
