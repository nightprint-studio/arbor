<script lang="ts">
  /**
   * The icon for a Java type kind — a filled disc with the letter that names it, the way
   * IntelliJ marks a class from an interface from an enum.
   *
   * A letter is the right mark here because the distinction being drawn is **nominal**: a
   * class is not a rounder or squarer kind of thing than an interface, so a glyph chosen to
   * suggest it is a picture of nothing. `C` / `I` / `E` / `R` / `@` are what these are
   * called, they are the letters IntelliJ trained everyone on, and they stay legible at the
   * 16px a tree row gives them, which few glyphs do.
   *
   * Drawn rather than imported: no icon set carries "letter in a ring" as a family, and
   * assembling one out of five unrelated glyphs is how the previous version ended up
   * unreadable.
   *
   * **On a filled row, set `--jki-color: currentColor` on the container.** The kind colours
   * are chosen to read against a panel background, and a selected row is not one: on a blue
   * fill the class ring goes muddy and the annotation ring — which is the accent — vanishes
   * into it completely. The colour is therefore a variable with the kind's hue as its
   * fallback, so a consumer that fills its selection takes the icon with it in one line
   * instead of the icon quietly disappearing at exactly the moment it is being pointed at.
   *
   * A **ring**, not a filled disc — which is IntelliJ's own choice and the better one. At the
   * 16px a tree row gives it, a solid disc is a blob of colour with a hole in it: the colour
   * shouts, the letter is what you actually read, and the two fight. An outline puts the
   * weight on the letter and lets a column of them sit quietly next to the file names.
   */

  /** Kind as the backend names it (`class`, `interface`, `enum`, `record`, `annotation`). */
  let { kind, title }: { kind: string; title?: string } = $props();

  /** Letter + colour per kind. The colours are the theme's semantic ones rather than
   *  invented hues, so the whole set re-tints with a theme instead of drifting out of it —
   *  and they are the assignments the rest of Bennu already uses for these kinds. */
  const MARKS: Record<string, { letter: string; color: string; label: string }> = {
    class:      { letter: 'C', color: 'var(--info)',                 label: 'Class' },
    interface:  { letter: 'I', color: 'var(--success)',              label: 'Interface' },
    enum:       { letter: 'E', color: 'var(--warning)',              label: 'Enum' },
    record:     { letter: 'R', color: 'var(--color-tag, #c792ea)',   label: 'Record' },
    annotation: { letter: '@', color: 'var(--accent)',               label: 'Annotation' },
  };

  const mark = $derived(
    MARKS[kind] ?? { letter: 'C', color: 'var(--text-muted)', label: 'Type' },
  );
  // `@` is a wider glyph than a capital letter — set slightly smaller so it sits inside the
  // ring instead of touching it.
  const fontSize = $derived(mark.letter === '@' ? 8 : 9);
</script>

<svg
  class="jki"
  viewBox="0 0 16 16"
  role="img"
  aria-label={title ?? mark.label}
>
  <!-- Radius leaves half the stroke inside the 16-unit box, so the ring is not clipped at
       the edges when the icon is scaled down. -->
  <circle
    cx="8"
    cy="8"
    r="6.4"
    fill="none"
    stroke="var(--jki-color, {mark.color})"
    stroke-width="1.4"
  />
  <text
    x="8"
    y="8"
    fill="var(--jki-color, {mark.color})"
    font-size={fontSize}
    font-weight="600"
    text-anchor="middle"
    dominant-baseline="central"
  >{mark.letter}</text>
</svg>

<style>
  .jki {
    /* Sized by the consumer's `font-size` (the tree row sizes its icon box from the same
       variable as its text), so this scales with the Appearance font setting like everything
       else in the row. */
    width: 1em;
    height: 1em;
    display: block;
    /* The UI sans, not the code font: a single letter in a monospace face sits off-centre
       inside a disc, because it is carrying the width of the widest character in the font. */
    font-family: var(--font-ui-sans);
  }
</style>
