<script lang="ts">
  /**
   * A product's **full icon** — the one the OS shows: rounded card, corner wedges, the
   * product's own palette. The same artwork the window wears in the taskbar.
   *
   * ## When to use this, and when to use {@link ProductMark} instead
   *
   * They are not two styles of one thing; they answer different questions.
   *
   * - **`ProductIcon`** — "which application is this?", asked somewhere with room to answer:
   *   a launcher card, a product tile, a welcome screen. It brings its own background and
   *   its own colours, which is the point — it is the app's face, and it should look the
   *   same here as it does in the taskbar.
   * - **`ProductMark`** — the same identity reduced to its initial, in `currentColor`, for
   *   inline chrome: a tab, a node in the launcher's circuit tree, anything under ~24px or
   *   anything that has to take the colour of what surrounds it. A full icon there would be
   *   a coloured card fighting the surface it sits on, at a size where none of its detail
   *   survives anyway.
   *
   * Rule of thumb: if the thing around it is monochrome, or smaller than about 24px, it
   * wants the mark.
   *
   * ## Why an `<img>` of an SVG
   *
   * Inlining would let it inherit CSS, which is exactly what we do NOT want here: the icon
   * owns its colours. `<img>` also keeps it out of the DOM as ~90 elements per card, and the
   * browser caches one file across every place it appears. Vector rather than the PNG so it
   * is right at whatever size the layout hands it.
   *
   * The files are published by `design/icons/rasterize.ps1` into `static/products/`, from
   * the SVGs in `design/icons/` — same source as the window icons.
   */

  interface Props {
    /** Product id (`corvus`, `bennu`, …) — the file name under `static/products/`. */
    id: string;
    size?: number;
    /** Accessible name. Omit beside a visible product name, where it would just repeat it. */
    title?: string;
  }

  let { id, size = 46, title }: Props = $props();
</script>

<img
  class="product-icon"
  src="/products/{id}.svg"
  width={size}
  height={size}
  alt={title ?? ''}
  aria-hidden={title ? undefined : 'true'}
  draggable="false"
/>

<style>
  .product-icon {
    display: block;
    flex-shrink: 0;
    /* The artwork already has its own rounded card, so no radius here — one would clip the
       corner wedges that make it part of the family. */
    user-select: none;
  }
</style>
