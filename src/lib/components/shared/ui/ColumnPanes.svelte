<script lang="ts">
  /**
   * Side-by-side panes for a master/detail dialog: a list (or two) on the left, the thing it
   * selects on the right.
   *
   * The shell of every "pick a revision, see what it changed" window — Local History, Git
   * history — so the column widths, the hairlines between panes and the error strip live here
   * once instead of in each dialog's stylesheet. It knows nothing about what the panes hold:
   * each pane is a snippet, usually a {@link PaneHeader} over its own body.
   *
   * `middle` is optional and sits between the two; the right pane takes whatever width is left.
   */
  import type { Snippet } from 'svelte';
  import Alert from './Alert.svelte';

  let {
    left,
    middle,
    right,
    error = '',
    leftWidth = '320px',
    middleWidth = '260px',
  }: {
    left: Snippet;
    middle?: Snippet;
    right: Snippet;
    /** A failure to show above the panes. Empty hides it. */
    error?: string;
    leftWidth?: string;
    middleWidth?: string;
  } = $props();
</script>

<div class="cp">
  {#if error}
    <div class="cp-error"><Alert variant="error" text={error} compact /></div>
  {/if}
  <div class="cp-cols">
    <div class="cp-pane" style:width={leftWidth} style:flex="none">{@render left()}</div>
    {#if middle}
      <div class="cp-pane" style:width={middleWidth} style:flex="none">{@render middle()}</div>
    {/if}
    <div class="cp-pane cp-grow">{@render right()}</div>
  </div>
</div>

<style>
  .cp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .cp-error { padding: 8px 10px 0; }
  .cp-cols { display: flex; flex: 1; min-height: 0; }
  .cp-pane { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .cp-pane + .cp-pane { border-left: 1px solid var(--border-subtle); }
  .cp-grow { flex: 1; }
</style>
