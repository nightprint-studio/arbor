<script lang="ts">
  /**
   * The line that says Imber is working, and on what — at the end of the thread while a run is open.
   *
   * An orb with a ripple, the phrase for what is happening now with a highlight travelling through it, and
   * the time and step quietly beside it. It is not a progress bar (a run's length is not known): it says the
   * run is alive and what it is doing. While a failed request waits to be sent again it shows one calm line
   * instead, with the verbs to skip the wait or stop.
   */
  import Button from '$lib/components/shared/ui/Button.svelte';
  import './imber-motion.css';

  let { label, meta = '', retry = null, detail = '', onRetryNow, onStop }: {
    /** The present-tense phrase, without the ellipsis ("Thinking", "Reading"). */
    label: string;
    /** Elapsed time, step — whatever is worth knowing while waiting. */
    meta?: string;
    /** The status line while the request is being sent again; replaces `label` and `meta`. */
    retry?: string | null;
    /** The provider's own words for the failure, as a tooltip on the retry line. */
    detail?: string;
    onRetryNow?: () => void;
    onStop?: () => void;
  } = $props();
</script>

<div class="activity" role="status" aria-live="polite">
  <span class="asst-orb" aria-hidden="true"></span>
  {#if retry}
    <span class="label retry" title={detail}>{retry}…</span>
    {#if onRetryNow}<Button variant="ghost" size="xs" tooltip="Skip the wait and send the request now" onclick={onRetryNow}>Retry now</Button>{/if}
    {#if onStop}<Button variant="ghost" size="xs" tooltip="Stop the run" onclick={onStop}>Stop</Button>{/if}
  {:else}
    <span class="asst-shimmer label">{label}…</span>
    {#if meta}<span class="meta">{meta}</span>{/if}
  {/if}
</div>

<style>
  .activity { display: flex; align-items: center; gap: 10px; padding: 2px 4px 2px 25px; }
  .label { font-size: var(--font-size-sm); font-weight: 500; }
  .retry { color: var(--warning); }
  .meta { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
</style>
