<script lang="ts">
  /**
   * A stream of lines with a colour per kind — what a process printed, or the unified timeline.
   *
   * The one place the row colours live (`LogStream` styles rows through global classes by design), so
   * the process log, the timeline and a scenario step's slice cannot disagree about what an error or a
   * request looks like.
   */
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';

  let {
    lines,
    classes = [],
    keys,
    emptyMessage = 'Nothing yet.',
    follow = $bindable(true),
  }: {
    lines: string[];
    /** The CSS class of each line, same order as `lines`. */
    classes?: string[];
    keys?: ReadonlyArray<string | number>;
    emptyMessage?: string;
    follow?: boolean;
  } = $props();
</script>

<div class="lv">
  <LogStream {lines} {keys} ansi={false} {emptyMessage} bind:autoScroll={follow} lineClass={(_l, i) => classes[i]} />
</div>

<style>
  .lv { height: 100%; min-height: 0; display: flex; flex-direction: column; }
  .lv :global(.log-line.line-stderr),
  .lv :global(.log-line.line-error) { color: var(--danger); }
  .lv :global(.log-line.line-http-req) { color: var(--accent); }
  .lv :global(.log-line.line-http-res) { color: var(--success); }
  .lv :global(.log-line.line-marker) { color: var(--text-muted); font-style: italic; }
</style>
