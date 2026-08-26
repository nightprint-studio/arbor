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
  interface Props {
    /** The value as it is now. Drawn struck through. */
    from: string;
    /** The value it becomes. */
    to: string;
    /** Render both in the monospace face — for identifiers, paths, anything code-shaped. */
    mono?: boolean;
  }

  let { from, to, mono = true }: Props = $props();
</script>

<span class="vc" class:mono>
  <span class="vc-from">{from}</span>
  <span class="vc-arrow" aria-hidden="true">→</span>
  <span class="vc-to">{to}</span>
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
</style>
