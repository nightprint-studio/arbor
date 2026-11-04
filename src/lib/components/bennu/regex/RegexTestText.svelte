<script lang="ts">
  /**
   * The Regex Tester's test-text box: a plain textarea with a mirror behind it that paints every match
   * and, inside it, the innermost capture group. The mirror carries the same text in the same font and
   * wrapping, transparent, so only its marks show through the transparent textarea — which is why
   * the two must scroll together and why neither may add padding the other lacks.
   *
   * There is no shared widget for "text field with ranges painted behind it"; this is the candidate
   * if a second place needs one.
   */
  import type { Segment } from './segments';

  let {
    value = $bindable(''),
    segments,
    placeholder = '',
    element = $bindable<HTMLTextAreaElement | undefined>(),
  }: {
    value?: string;
    segments: Segment[];
    placeholder?: string;
    element?: HTMLTextAreaElement;
  } = $props();

  let mirror = $state<HTMLElement>();

  function syncScroll() {
    if (mirror && element) {
      mirror.scrollTop = element.scrollTop;
      mirror.scrollLeft = element.scrollLeft;
    }
  }
</script>

<div class="rt-wrap">
  <div class="rt-sizer" aria-hidden="true">{value}{'\n'}</div>
  <div class="rt-mirror" bind:this={mirror} aria-hidden="true">{#each segments as s, i (i)}<span class={s.match < 0 ? 'rt-plain' : `rt-m${s.match % 2}`} class:rt-on={s.active} data-g={s.group > 0 ? ((s.group - 1) % 6) + 1 : undefined}>{s.text}</span>{/each}{'\n'}</div>
  <textarea
    class="rt-input"
    bind:this={element}
    bind:value
    onscroll={syncScroll}
    spellcheck="false"
    autocapitalize="off"
    autocomplete="off"
    {placeholder}
    aria-label="Test text"
  ></textarea>
</div>

<style>
  .rt-wrap {
    position: relative;
    flex: 0 1 auto;
    max-height: 62%;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-base);
    overflow: hidden;
  }
  /* Gives the box the height of its text (down to a floor) — the mirror and the field are laid over it. */
  .rt-sizer {
    visibility: hidden;
    min-height: 112px;
    padding: 8px 10px;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 4;
    box-sizing: border-box;
  }
  .rt-wrap:focus-within { border-color: var(--accent); }

  .rt-mirror,
  .rt-input {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: 8px 10px;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 4;
    box-sizing: border-box;
    border: 0;
    overflow: auto;
  }
  .rt-mirror { color: transparent; pointer-events: none; }
  .rt-input {
    resize: none;
    outline: none;
    background: transparent;
    color: var(--text-primary);
  }

  .rt-m0 { background: color-mix(in srgb, var(--accent) 26%, transparent); border-radius: 2px; }
  .rt-m1 { background: color-mix(in srgb, var(--info, var(--accent)) 26%, transparent); border-radius: 2px; }
  /* A capture group takes its own tint over the match's, the same six the Matches table uses. */
  [data-g='1'] { background: color-mix(in srgb, var(--rx-g1) 45%, transparent); }
  [data-g='2'] { background: color-mix(in srgb, var(--rx-g2) 45%, transparent); }
  [data-g='3'] { background: color-mix(in srgb, var(--rx-g3) 45%, transparent); }
  [data-g='4'] { background: color-mix(in srgb, var(--rx-g4) 45%, transparent); }
  [data-g='5'] { background: color-mix(in srgb, var(--rx-g5) 45%, transparent); }
  [data-g='6'] { background: color-mix(in srgb, var(--rx-g6) 45%, transparent); }
  .rt-on { outline: 1px solid var(--text-primary); outline-offset: -1px; border-radius: 2px; }
</style>
