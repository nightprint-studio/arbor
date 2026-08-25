<script lang="ts">
  /**
   * The status strip along the bottom of a dock panel — the sibling of
   * {@link BottomPanelHeader}.
   *
   * ## Why a panel gets its own footer when the window already has one
   *
   * Because they answer about different things. The window's status bar belongs to
   * the **window**: the connection, the project, findings — facts that are true
   * wherever you are looking. A panel's footer belongs to **that panel**: how many
   * rows this result has, how long it took, whether it can be written to.
   *
   * Putting the second kind in the window's bar is what this exists to stop. It
   * reads fine with one tab open and becomes a puzzle with four: a row count in
   * window chrome makes the reader ask *which tab is this about?*, and the answer is
   * nowhere on screen. Two feet lower, inside the panel, the answer is "the panel
   * you are looking at" — by position, with nothing to work out.
   *
   * ## Status only
   *
   * Header for verbs, footer for facts. A control here would undo the separation
   * that makes both scannable — you would be back to reading a strip to find out
   * whether each thing is something you can press. The one exception the shape
   * allows is a fact that is *also* a shortcut to whatever explains it, which is the
   * rule the window's bar already follows.
   *
   * ## It costs height, and that is the trade
   *
   * A dock panel is short, and this is another ~26px of chrome. It buys back the
   * header's horizontal budget, which is the scarcer of the two once a panel has a
   * tab strip and three or four actions — and it puts each kind of information
   * where it is looked for rather than where it fitted.
   */
  import type { Snippet } from 'svelte';

  interface Props {
    /** Facts, from the left. */
    children?: Snippet;
    /** Facts pinned to the right, after the spacer. */
    trailing?: Snippet;
  }

  let { children, trailing }: Props = $props();
</script>

<div class="bp-footer">
  {#if children}{@render children()}{/if}
  {#if trailing}
    <span class="bp-spacer"></span>
    {@render trailing()}
  {/if}
</div>

<style>
  .bp-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    min-height: 26px;
    padding: 0 10px 0 12px;
    background: var(--bg-base);
    /* A top border rather than a background of its own: the strip is part of the
       panel, not a thing sitting under it. */
    border-top: 1px solid var(--border-subtle);
    flex-shrink: 0;
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    /* Facts do not wrap onto a second line and do not push the panel wider; a strip
       that grew would take height from the very thing it is describing. */
    overflow: hidden;
    white-space: nowrap;
  }

  .bp-spacer { flex: 1; min-width: 8px; }
</style>
