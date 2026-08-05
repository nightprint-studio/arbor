<script lang="ts">
  /**
   * The console: a stream of interpreted output, however long it gets.
   *
   * One component rather than three. Run, Build and Tests were each rendering the same
   * `RunLogLine[]` with their own copy of the row markup, the stream colours and the
   * scroll-to-bottom effect — three places to fix a bug, and three that had already drifted
   * (only Run stopped following when you scrolled up to read something).
   *
   * ## Why it is virtualised
   *
   * The buffer is capped at a few thousand lines, and a few thousand lines is not a small
   * DOM: every line is a row plus one element per interpreted piece — the level, the
   * timestamp, the thread, the logger, each frame of a trace — so a full buffer was on the
   * order of twenty thousand nodes, all of them live, all of them laid out again whenever the
   * panel resized. {@link VirtualTextView} keeps only what is on screen (plus an overscan),
   * which makes the cost of scrolling a function of the viewport rather than of how long the
   * program has been talking.
   *
   * The price it charges is **fixed-height rows**: the virtualiser knows where line *n* is
   * because every line is the same height, and that is what makes the arithmetic exact rather
   * than a guess that drifts as you scroll. So a long line **scrolls sideways** instead of
   * wrapping — the trade a terminal makes, and the reason `white-space` here is `pre`.
   *
   * The height is measured rather than hard-coded: the app's font scale is a user setting, and
   * a constant would put the rows and the virtualiser's idea of the rows out of step the
   * moment someone changed it.
   *
   * A consequence worth knowing: a select-all copies the lines that are *rendered*, not the
   * whole buffer. Anything that needs the entire transcript has to read it from the store.
   */
  import VirtualTextView from '$lib/components/shared/ui/VirtualTextView.svelte';
  import LogLine from '$lib/components/shared/ui/LogLine.svelte';
  import type { RunLogLine } from '$lib/stores/bennu/run.svelte';
  import { openLogLink } from './log-link';

  interface Props {
    /** The buffer to show, oldest first. */
    lines: RunLogLine[];
    /** Shown centred when there is nothing yet. */
    emptyMessage?: string;
    /** Extra class on the viewport. */
    class?: string;
  }

  let { lines, emptyMessage = 'No output.', class: klass = '' }: Props = $props();

  /** The virtualiser takes plain strings; the interpreted line is looked up by index in the
   *  row snippet, so nothing is copied but the text. */
  const texts = $derived(lines.map((l) => l.text));

  /**
   * Follow the tail only while the reader is AT the bottom — scrolling up to read something
   * is a statement that you want to stay there, and a console that yanks you back down on the
   * next line is unusable on a chatty program. {@link VirtualTextView} re-arms it when you
   * scroll back down.
   */
  let follow = $state(true);

  /** The measured height of one row, in px. Zero until the probe lands — see the fallback at
   *  the call site, which only ever shows for the first frame. */
  let rowH = $state(0);

  /**
   * Whether a stderr line should be spared the red.
   *
   * stderr-is-red is a good default for a stream nobody interprets. Once the lines ARE
   * interpreted it becomes a lie about the common case: Tomcat, and any `java.util.logging`
   * default, writes its ordinary `INFO` chatter to stderr, and a console that paints a hundred
   * startup lines red has no way left to show the one that is actually wrong.
   */
  function isCalm(l: RunLogLine): boolean {
    return l.stream === 'err' && (l.level === 'info' || l.level === 'debug' || l.level === 'trace');
  }

  function rowClass(_text: string, index: number): string {
    const l = lines[index];
    if (!l) return '';
    return `stream-${l.stream}${isCalm(l) ? ' calm' : ''}`;
  }
</script>

<!-- Off-layout, and the same type as a row: what it measures is what a row will be, including
     whatever the user has done to the font scale. -->
<span class="bc-probe" bind:clientHeight={rowH} aria-hidden="true">0</span>

<VirtualTextView
  class="bc {klass}"
  lines={texts}
  lineHeight={rowH || 17}
  bind:follow
  {rowClass}
  {emptyMessage}
>
  {#snippet line({ index })}
    {@const l = lines[index]}
    <!-- No whitespace between the caret and the line: the row is `pre`, and a newline in the
         template would arrive on the page as a space the program never printed. -->
    {#if l}{#if l.stream === 'in'}<span class="bc-caret">&gt;</span>{/if}<LogLine
        text={l.text}
        pieces={l.pieces}
        level={l.level}
        onopen={openLogLink}
      />{/if}
  {/snippet}
</VirtualTextView>

<style>
  .bc-probe {
    position: absolute;
    top: 0;
    left: -9999px;
    display: block;
    visibility: hidden;
    pointer-events: none;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }

  /* The viewport. The rows are rendered by VirtualTextView, so their own rules are `:global`
     through the class it puts on them — `rowClass` exists for exactly this. */
  .bc-caret {
    color: var(--accent);
    margin-right: 6px;
    user-select: none;
  }

  :global(.vtv.bc) {
    /* The Run panel puts this in a ROW beside the debugger's columns, and a flex item without
       it refuses to shrink below its widest line — one long line would push the columns off. */
    min-width: 0;
    padding: 6px 0;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    user-select: text;
  }
  /* Padding on the row rather than on the viewport, so it survives a sideways scroll. */
  :global(.vtv.bc .vtv-row) {
    padding: 0 12px;
    color: var(--text-secondary);
  }
  :global(.vtv.bc .vtv-row.stream-err) { color: var(--error); }
  /* …unless the line said what it was, and what it was is routine — see `isCalm`. */
  :global(.vtv.bc .vtv-row.stream-err.calm) { color: var(--text-secondary); }
  :global(.vtv.bc .vtv-row.stream-meta) { color: var(--text-muted); font-style: italic; }
  /* What you typed, marked the way a shell marks it — so a transcript reads as a conversation
     rather than as the program talking to itself. */
  :global(.vtv.bc .vtv-row.stream-in) { color: var(--text-primary); }
</style>
