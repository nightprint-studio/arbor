<script lang="ts">
  /**
   * A horizontal strip that **floats inside** a panel rather than spanning it.
   *
   * The house style, in one widget. Arbor's layout follows IntelliJ's New UI: chrome
   * sits on `--bg-elevated`, content on `--bg-base`, and the separation between them
   * is drawn by a **gap that lets the background through** — never by a border line.
   * A strip with a full-width background and a `border-bottom` reads as a second
   * header welded under the first; the same strip inset a few pixels with rounded
   * corners reads as a thing sitting *on* the panel, which is what it is.
   *
   * ## Why it is shared rather than a rule people re-type
   *
   * Because it was already being re-typed. An editor toolbar and a colour legend are
   * not the same component and have no business sharing one, but they are the same
   * *surface* — and every copy of `margin: 3px 6px; border-radius; background` is
   * another chance for one of them to drift by a pixel or forget the radius. One
   * widget means the look is defined once and a new strip inherits it by existing.
   *
   * ## What it is not
   *
   * Not a container with opinions about its contents: it lays nothing out beyond a
   * row with a gap, and every child decides its own size. Anything that needs a
   * title, a count or a close button wants `BottomPanelHeader` — that is a header,
   * this is a surface.
   */
  import type { Snippet } from 'svelte';

  interface Props {
    children: Snippet;
    /**
     * Render as a toolbar for assistive technology.
     *
     * A strip of controls is a toolbar; a strip of labels is not, and announcing a
     * legend as one would promise a keyboard user a control group that is not there.
     * So it is opt-in, and it brings `tabindex="-1"` with it — a `role="toolbar"`
     * has to be reachable as a unit for a screen reader to announce what the
     * controls inside it belong to.
     */
    toolbar?: boolean;
    /** What the strip is, announced. Required alongside `toolbar`. */
    ariaLabel?: string;
    /** Space between children. Defaults to the toolbar's 4px. */
    gap?: number;
  }

  let { children, toolbar = false, ariaLabel, gap = 4 }: Props = $props();
</script>

<!-- The three attributes arrive together or not at all, spread from one object.
     Written as separate conditional attributes, Svelte's a11y pass cannot tell that
     `tabindex` only exists where `role` does — it sees a plain `<div>` carrying a
     tabindex and warns about a non-interactive element being focusable. A spread is
     opaque to that check *and* states the invariant honestly: a toolbar has a role,
     a label and a way to be reached, and a strip of labels has none of the three. -->
<div
  class="fb"
  style:--fb-gap={`${gap}px`}
  {...toolbar ? { role: 'toolbar', tabindex: -1, 'aria-label': ariaLabel } : {}}
>
  {@render children()}
</div>

<style>
  .fb {
    display: flex;
    align-items: center;
    gap: var(--fb-gap, 4px);
    flex-shrink: 0;
    /* The whole look. `3px 6px` is what the editor's toolbar has always floated by,
       and matching it exactly is the point — two strips inset differently read as a
       mistake even when nobody can say which one is wrong. */
    margin: 3px 6px;
    padding: 0 8px;
    min-height: 32px;
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
    /* No border, deliberately: the gap around it is the separation, and adding a
       line as well draws the same boundary twice. */
    /* Contents wrap onto a second line rather than being clipped — a legend of
       eight tables on a narrow panel grows, and a table name cut in half is worse
       than a strip one row taller. */
    flex-wrap: wrap;
  }
</style>
