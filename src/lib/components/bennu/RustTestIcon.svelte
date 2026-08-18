<script lang="ts">
  /**
   * The mark for Rust's Tests tool window — a toothed ring with a check inside it.
   *
   * ## Why not a flask
   *
   * A flask says *experiment*, and the panel says "the tests this project declares". It is also
   * the wrong register: `#[test]` is not laboratory glassware, it is the thing that has to be
   * green before you push. Java's button gets JUnit's own brand mark for the same reason — a
   * button that names a product should show the product.
   *
   * Rust has no test-framework brand to borrow (libtest ships with the compiler and has no mark),
   * so this is drawn rather than imported: the two ideas that *do* belong to it, composed.
   *
   * - The **toothed ring** is Rust's gear, reduced to what survives at 16px. It is a dashed
   *   stroke, not eight drawn teeth, because eight teeth at this size is a grey smudge and a
   *   dashed circle reads as a cog the moment it is small.
   * - The **check** is what makes it *tests* rather than *Rust*. Without it this would be the
   *   Cargo button, which is two rows above in the same rail.
   *
   * That is also why the ring is thin and open while Cargo's is lucide's solid many-toothed
   * `Cog`: side by side in a vertical rail the two must not be the same silhouette, and the
   * difference here is weight plus what sits in the middle — a hub there, a verdict here.
   *
   * Monochrome, on `currentColor`, because every icon on the rail is drawn in one colour so the
   * surface reads as one surface. Takes `size` (not `width`/`height`) so it drops into any icon
   * slot typed for lucide — which is what the activity rail's `icon` is.
   */
  let { size = 18 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <!-- The gear, suggested. Radius leaves half the stroke inside the box so nothing clips when the
       icon is scaled down; the dash pattern is tuned so the gaps stay visible at 16px and the ring
       still closes cleanly at 24. -->
  <circle cx="8" cy="8" r="6.2" stroke-width="1.35" stroke-dasharray="2.3 1.55" />
  <!-- The verdict. Heavier than the ring: it is the part that has to be legible first. -->
  <path d="M5.1 8.2 7.05 10.2 10.9 6.05" stroke-width="1.7" />
</svg>

<style>
  svg {
    display: block;
    flex-shrink: 0;
  }
</style>
