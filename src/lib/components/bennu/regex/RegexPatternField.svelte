<script lang="ts">
  /**
   * The pattern field: a single-line input drawn over a mirror that paints the regex syntax (groups,
   * classes, quantifiers, escapes) and underlines what the parser rejects, with the message on hover.
   *
   * It also reports where the caret is and takes a range to emphasise, which is how the Explain tab and
   * the pattern stay in step: hover a row and its token lights up here; move the caret here and the row
   * it is in lights up there.
   *
   * No shared widget draws coloured text inside an input; this is the candidate if a second place needs one.
   */
  import { paintSegments } from './pattern-paint';
  import type { RegexSpan } from './tokens';

  let {
    value = $bindable(''),
    spans,
    emphasis = null,
    placeholder = '',
    element = $bindable<HTMLInputElement | undefined>(),
    oncaret,
  }: {
    value?: string;
    spans: RegexSpan[];
    /** A range to draw attention to (the token being hovered in the explanation). */
    emphasis?: { from: number; to: number } | null;
    placeholder?: string;
    element?: HTMLInputElement;
    oncaret?: (pos: number) => void;
  } = $props();

  let mirror = $state<HTMLElement>();
  const segments = $derived(paintSegments(value, spans, emphasis));

  function sync() {
    if (mirror && element) mirror.scrollLeft = element.scrollLeft;
    oncaret?.(element?.selectionStart ?? 0);
  }
</script>

<div class="pf">
  <div class="pf-mirror" bind:this={mirror} aria-hidden="true">{#each segments as s, i (i)}<span class="t-{s.cls || 'plain'}" class:on={s.active} class:bad={s.message !== undefined} title={s.message}>{s.text}</span>{/each}</div>
  <input
    class="pf-input"
    bind:this={element}
    bind:value
    oninput={sync}
    onkeyup={sync}
    onclick={sync}
    onselect={sync}
    onscroll={sync}
    spellcheck="false"
    autocapitalize="off"
    autocomplete="off"
    {placeholder}
    aria-label="Regular expression"
  />
</div>

<style>
  .pf {
    position: relative;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-base);
    overflow: hidden;
  }
  .pf:focus-within { border-color: var(--accent); }

  .pf-mirror,
  .pf-input {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: 0 10px;
    box-sizing: border-box;
    border: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-md);
    line-height: 28px;
    white-space: pre;
    letter-spacing: normal;
  }
  .pf-mirror { overflow: hidden; pointer-events: none; color: var(--syntax-string, var(--text-primary)); }
  .pf-input {
    width: 100%;
    background: transparent;
    outline: none;
    color: transparent;
    caret-color: var(--text-primary);
  }
  .pf-input::placeholder { color: var(--text-muted); }
  .pf-input::selection { background: color-mix(in srgb, var(--accent) 35%, transparent); color: transparent; }

  .t-plain { color: var(--text-primary); }
  .t-keyword { color: var(--syntax-keyword, #cc7832); font-weight: 600; }
  .t-type { color: var(--syntax-type, #4d9be6); }
  .t-operator { color: var(--syntax-function, #ffc66d); font-weight: 600; }
  .t-constant { color: var(--syntax-constant, #9876aa); }
  .t-field { color: var(--syntax-field, #9876aa); }
  .t-escape { color: var(--syntax-escape, #cc7832); }
  .t-annotation { color: var(--syntax-annotation, #bbb529); }
  .t-comment { color: var(--syntax-comment, #808080); font-style: italic; }
  .on { background: color-mix(in srgb, var(--accent) 30%, transparent); border-radius: 2px; }
  .bad { text-decoration: underline wavy var(--danger, #d16969); text-underline-offset: 3px; }
</style>
