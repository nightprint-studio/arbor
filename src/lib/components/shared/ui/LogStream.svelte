<script lang="ts">
  /**
   * Generic streaming-output viewport.
   *
   * Renders a scrollable list of lines with auto-follow (tail-f), ANSI →
   * HTML conversion, and a per-line CSS class hook.  Used by
   *   - JobOutputPanel        (job stdout/stderr)
   *   - PluginLogsPanel       (arbor.log.* messages)
   *   - PipelineRunDetailModal / JobsPanel …any future log surface
   *
   * Thin wrapper over {@link VirtualTextView}: only the visible window of lines
   * is ever in the DOM, so multi-thousand-line logs stay fluid. Each row keeps
   * the `.log-line` class (+ the caller `lineClass`) so existing
   * `:global(.log-line …)` styling rules keep matching.
   *
   * Header / toolbar / extra action buttons are the caller's responsibility:
   * compose this widget inside `<PanelShell>` (or anything else) and surface
   * Follow / Copy / Clear / filters in your own action snippet.  Two-way
   * follow state is exposed via `bind:autoScroll` so the caller's "Follow"
   * toggle stays in sync with manual scroll-up pauses.
   *
   * NOTE: rows are fixed-height (no soft-wrap) so virtualization stays exact —
   * long lines scroll horizontally rather than wrapping.
   */
  import { ansiToHtml } from '$lib/utils/ansi-to-html';
  import VirtualTextView from './VirtualTextView.svelte';

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
     *  `lines`. Falls back to index when omitted. */
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

  let view = $state<VirtualTextView | undefined>();

  function rowClass(line: string, idx: number): string {
    return `log-line ${lineClass ? (lineClass(line, idx) ?? '') : ''}`.trim();
  }

  /** Scroll to the bottom and re-enable auto-follow.  Exported for a Follow /
   *  Jump-to-end button. */
  export function scrollToBottom() {
    view?.scrollToBottom();
  }
</script>

<VirtualTextView
  bind:this={view}
  {lines}
  {keys}
  {emptyMessage}
  {waiting}
  {waitingMessage}
  bind:follow={autoScroll}
  lineHeight={20}
  role="log"
  class="log-stream"
  {rowClass}
>
  {#snippet line({ text, index })}
    {#if ansi}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      <span class="log-line-content">{@html ansiToHtml(text)}</span>
    {:else if lineHtml}
      {@const html = lineHtml(text, index)}
      {#if html !== undefined}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        <span class="log-line-content">{@html html}</span>
      {:else}
        <span class="log-line-content">{text}</span>
      {/if}
    {:else}
      <span class="log-line-content">{text}</span>
    {/if}
  {/snippet}
</VirtualTextView>

<style>
  /* The scroll viewport is VirtualTextView's element (we pass `class`). The
     default line colour lives on the container so consumer `:global(.log-line …)`
     rules (higher specificity) keep overriding per-line colours as before. */
  :global(.log-stream) {
    padding: 6px 0;
    user-select: text;
    cursor: text;
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-secondary);
    background: var(--bg-base);
  }

  /* Row wrapper (`.log-line`) padding. Fixed-height rows (set by
     VirtualTextView) → no soft-wrap; long lines scroll horizontally. */
  :global(.log-stream .log-line) {
    padding: 0 14px;
  }
  :global(.log-stream .log-line-content) {
    white-space: pre;
  }
</style>
