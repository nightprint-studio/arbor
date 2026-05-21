<script lang="ts">
  /**
   * Generic streaming-output viewport.
   *
   * Renders a scrollable list of lines with auto-follow (tail-f), ANSI →
   * HTML conversion, and a per-line CSS class hook.  Used by
   *   - JobOutputPanel        (job stdout/stderr)
   *   - PluginLogsPanel       (arbor.log.* messages)
   *   - …any future log surface
   *
   * Header / toolbar / extra action buttons are the caller's responsibility:
   * compose this widget inside `<PanelShell>` (or anything else) and surface
   * Follow / Copy / Clear / filters in your own action snippet.  Two-way
   * follow state is exposed via `bind:autoScroll` so the caller's "Follow"
   * toggle stays in sync with manual scroll-up pauses.
   */
  import { ansiToHtml } from '$lib/utils/ansi-to-html';

  interface Props {
    /** Flat list of output lines.  May contain ANSI escape sequences. */
    lines: string[];
    /** Convert ANSI escapes to coloured spans (default true). */
    ansi?: boolean;
    /** Returns an extra CSS class to apply per line (e.g. 'line-stderr'). */
    lineClass?: (line: string, idx: number) => string | undefined;
    /** Optional pre-formatted HTML for the line. When provided (and `ansi`
     *  is false) it is rendered via {@html} instead of the raw text — lets
     *  callers tokenise timestamps / levels / tags into coloured spans. The
     *  caller is responsible for escaping any user content they interpolate. */
    lineHtml?: (line: string, idx: number) => string | undefined;
    /** Stable per-row keys for `{#each}` reconciliation. Length must match
     *  `lines`. When provided, Svelte recycles existing DOM nodes for rows
     *  that survive a filter / search change instead of re-rendering the
     *  whole list — major win for log viewers that rebuild `lines` on every
     *  predicate change. Falls back to index when omitted. */
    keys?: ReadonlyArray<string | number>;
    /** Shown when `lines` is empty and `waiting` is false. */
    emptyMessage?: string;
    /** Show the waiting indicator instead of the empty message. */
    waiting?: boolean;
    /** Text shown alongside the waiting indicator. */
    waitingMessage?: string;
    /** Tail-follow mode.  Two-way bindable so callers can drive a "Follow"
     *  button. Set to false when the user manually scrolls up. */
    autoScroll?: boolean;
  }

  let {
    lines,
    ansi            = true,
    lineClass,
    lineHtml,
    keys,
    emptyMessage    = 'No output captured.',
    waiting         = false,
    waitingMessage  = 'Waiting for output…',
    autoScroll      = $bindable(true),
  }: Props = $props();

  let scrollEl = $state<HTMLDivElement | null>(null);

  // Re-pin to bottom whenever a new line lands while auto-scroll is on.
  $effect(() => {
    // Reactive dep: recount lines on every push.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const _ = lines.length;
    if (autoScroll && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  function onScroll() {
    if (!scrollEl) return;
    const atBottom =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight < 40;
    autoScroll = atBottom;
  }

  /** Scroll to the bottom and re-enable auto-follow.  Exported so callers
   *  can wire it to a Follow / Jump-to-end button. */
  export function scrollToBottom() {
    if (!scrollEl) return;
    scrollEl.scrollTop = scrollEl.scrollHeight;
    autoScroll = true;
  }
</script>

<div
  class="log-stream"
  bind:this={scrollEl}
  onscroll={onScroll}
  role="log"
  aria-live="polite"
>
  {#if lines.length === 0}
    <div class="log-empty">
      {#if waiting}
        <span class="log-waiting">{waitingMessage}</span>
      {:else}
        <span>{emptyMessage}</span>
      {/if}
    </div>
  {:else}
    {#each lines as line, i (keys && keys.length === lines.length ? keys[i] : i)}
      {@const extra = lineClass ? lineClass(line, i) : undefined}
      {@const html  = lineHtml  ? lineHtml(line, i)  : undefined}
      {#if ansi}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        <div class="log-line {extra ?? ''}">{@html ansiToHtml(line)}</div>
      {:else if html !== undefined}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        <div class="log-line {extra ?? ''}">{@html html}</div>
      {:else}
        <div class="log-line {extra ?? ''}">{line}</div>
      {/if}
    {/each}
  {/if}
</div>

<style>
  .log-stream {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 0;
    user-select: text;
    cursor: text;
    font-family: var(--font-code);
    font-size: 12px;
    background: var(--bg-base);
  }

  .log-line {
    padding: 1px 14px;
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.6;
    color: var(--text-secondary);
  }

  .log-empty {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  .log-waiting {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    justify-content: center;
  }
</style>
