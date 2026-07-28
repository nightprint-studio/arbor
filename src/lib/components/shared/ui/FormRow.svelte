<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    /**
     * Give the control the row's remaining width instead of sizing it to its
     * own content.
     *
     * Off by default, because the common control is a toggle or a short select
     * and stretching those would leave a switch marooned at the end of a long
     * empty strip. Set it for anything that holds *text the user has to read* —
     * a picker over a schema's tables, a select whose options are sentences. The
     * default costs nothing until the content happens to be short or empty, at
     * which point the control collapses to the width of its own chevron.
     */
    wideControl?: boolean;
    /** Right-aligned control (toggle, input, select, button…). */
    children: Snippet;
  }

  let { label, description, wideControl = false, children }: Props = $props();
</script>

<div class="fr-row">
  <!-- Header: label on the left, control on the right.
       Keeping the label compact (no description here) prevents narrow controls
       like a Toggle from being stuck in a half-row "second column" while the
       description tries to wrap into a tiny strip on the left. -->
  <div class="fr-header" class:fr-wide-header={wideControl}>
    <span class="fr-title">{label}</span>
    <div class="fr-control" class:fr-wide={wideControl}>
      {@render children()}
    </div>
  </div>

  {#if description}
    <p class="fr-desc">{description}</p>
  {/if}
</div>

<style>
  /* `fr-` prefixed class names so SettingsPanel's `.content :global(.row-title)`
     overrides (intended for legacy direct usage in CacheSection /
     RecoverySection) don't clobber FormRow's typography.  Mirrors the
     `.feat-row` styling from ExperimentalSection: same padding, title size,
     description tone — every settings row reads at the same visual weight. */
  .fr-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .fr-row:last-child { border-bottom: none; }

  .fr-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-width: 0;
  }

  .fr-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
    min-width: 0;
  }

  .fr-control {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  /* Opt-in: the control takes what the title leaves. `min-width: 0` lets it
     shrink past its content instead of pushing the title out of the row. */
  .fr-control.fr-wide { flex: 1 1 auto; min-width: 0; }
  /* …and the title stops competing for it: without this the two split the row
     evenly and a one-word label reserves half of it. */
  .fr-header.fr-wide-header .fr-title { flex: 0 1 auto; }

  .fr-desc {
    margin: 0;
    font-size: 0.77rem;
    color: var(--text-secondary);
    line-height: 1.55;
  }
</style>
