<script lang="ts">
  /**
   * The context column of a find list: what the selected hit looks like in place.
   *
   * Two bodies, one frame. With a `language` the text goes to the real read-only editor
   * (`CodePreview`) so it is coloured exactly as the buffer is; without one, `segments` are
   * rendered as plain wrapped text with the matches marked — for prose, where a code grammar
   * would be noise. The host loads the text; this only presents it.
   */
  import type { Snippet } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import CodePreview from '../CodePreview.svelte';
  import type { LanguageDescriptor } from '../code-editor';
  import type { FindSegment } from './find-types';

  interface Props {
    /** The line over the preview (path, note title). */
    label?: string;
    /** Hover text for the label; defaults to the label. */
    labelTooltip?: string;
    /** Replaces the body with a message ("Reading…", "can't be previewed"). */
    note?: string | null;
    /** The whole text, for code mode. */
    text?: string;
    /** Code mode: the grammar to colour with. */
    language?: LanguageDescriptor;
    /** Code mode: 1-based line to band and scroll to. */
    activeLine?: number | null;
    /** Plain mode: the text, pre-split into matched / unmatched runs. */
    segments?: FindSegment[];
    /** Plain mode: a line under the text (e.g. "cut at N characters"). */
    footnote?: string | null;
    /** Controls at the right of the label row. */
    actions?: Snippet;
  }

  let {
    label = '',
    labelTooltip,
    note = null,
    text = '',
    language,
    activeLine = null,
    segments,
    footnote = null,
    actions,
  }: Props = $props();
</script>

<div class="fp">
  {#if label || actions}
    <div class="fp-head">
      <span class="fp-label" use:tooltip={labelTooltip ?? label}>{label}</span>
      {#if actions}<span class="fp-actions">{@render actions()}</span>{/if}
    </div>
  {/if}

  {#if note}
    <p class="fp-note">{note}</p>
  {:else if language}
    <div class="fp-body">
      <CodePreview {text} {language} {activeLine} />
    </div>
  {:else}
    <div class="fp-body fp-scroll">
      <pre class="fp-text">{#each segments ?? [] as s, i (i)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</pre>
      {#if footnote}<p class="fp-foot">{footnote}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .fp { min-height: 0; min-width: 0; height: 100%; display: flex; flex-direction: column; background: var(--bg-base); }

  .fp-head {
    flex-shrink: 0; display: flex; align-items: center; gap: 8px;
    padding: 6px 12px; min-height: 30px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fp-label {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .fp-actions { display: inline-flex; align-items: center; flex-shrink: 0; }

  /* `CodePreview` owns its gutter, colouring and scrolling — this is only the box it fills. */
  .fp-body { flex: 1; min-height: 0; }
  .fp-scroll { overflow: auto; }
  .fp-note { padding: 14px; font-size: var(--font-size-sm); color: var(--text-muted); }

  .fp-text {
    margin: 0; padding: 10px 12px;
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.65;
    color: var(--text-secondary);
    white-space: pre-wrap; overflow-wrap: anywhere; user-select: text;
  }
  .fp-foot { margin: 0; padding: 0 12px 12px; font-size: var(--font-size-2xs); color: var(--text-muted); }

  mark {
    padding: 0 1px; border-radius: 2px;
    background: color-mix(in srgb, var(--warning) 32%, transparent);
    color: var(--text-primary);
  }
</style>
