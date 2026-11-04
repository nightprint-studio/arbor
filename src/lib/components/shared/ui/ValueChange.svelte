<script lang="ts">
  /**
   * One value becoming another — `oldName → newName`, struck through and in the accent.
   *
   * Small, but it is a **statement about a change**, and it was being spelled out span by span
   * wherever a preview showed one: the same three elements, the same arrow, and three chances for
   * the strike-through and the colour to drift apart between two screens that describe the same
   * operation.
   *
   * Not the same thing as `TextDiff`, which compares two *documents* line by line. This is a single
   * value, inline, in a row.
   */
  import { diffNames } from '$lib/utils/name-diff';

  interface Props {
    /** The value as it is now. Drawn struck through. */
    from: string;
    /** The value it becomes. */
    to: string;
    /** Render both in the monospace face — for identifiers, paths, anything code-shaped. */
    mono?: boolean;
    /** Mark the characters that differ: the text the two values share at the ends is drawn plain and
     *  only what is between is struck / tinted. For two spellings of one name (`get_x` → `getX`),
     *  where the eye otherwise compares them letter by letter. */
    highlight?: boolean;
  }

  let { from, to, mono = true, highlight = false }: Props = $props();

  const diff = $derived(highlight ? diffNames(from, to) : null);
</script>

<span class="vc" class:mono class:hl={!!diff}>
  {#if diff}
    <span class="vc-from">{diff.prefix}<mark class="vc-del">{diff.removed}</mark>{diff.suffix}</span>
    <span class="vc-arrow" aria-hidden="true">→</span>
    <span class="vc-to">{diff.prefix}<mark class="vc-add">{diff.added}</mark>{diff.suffix}</span>
  {:else}
    <span class="vc-from">{from}</span>
    <span class="vc-arrow" aria-hidden="true">→</span>
    <span class="vc-to">{to}</span>
  {/if}
</span>

<style>
  .vc {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .mono .vc-from,
  .mono .vc-to {
    font-family: var(--font-mono);
  }
  .vc-from {
    color: var(--text-secondary);
    text-decoration: line-through;
  }
  .vc-arrow {
    color: var(--text-muted);
  }
  .vc-to {
    color: var(--success);
  }
  /* With `highlight`: the whole name is plain-coloured and only the changed run is marked. */
  .hl .vc-from { text-decoration: none; }
  .hl .vc-to { color: var(--text-primary); }
  .vc-del,
  .vc-add {
    background: transparent;
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }
  .vc-del {
    background: color-mix(in srgb, var(--danger) 22%, transparent);
    text-decoration: line-through;
  }
  .vc-add {
    background: color-mix(in srgb, var(--success) 24%, transparent);
    font-weight: 600;
  }
</style>
