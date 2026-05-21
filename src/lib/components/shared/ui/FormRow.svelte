<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    /** Right-aligned control (toggle, input, select, button…). */
    children: Snippet;
  }

  let { label, description, children }: Props = $props();
</script>

<div class="fr-row">
  <!-- Header: label on the left, control on the right.
       Keeping the label compact (no description here) prevents narrow controls
       like a Toggle from being stuck in a half-row "second column" while the
       description tries to wrap into a tiny strip on the left. -->
  <div class="fr-header">
    <span class="fr-title">{label}</span>
    <div class="fr-control">
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

  .fr-desc {
    margin: 0;
    font-size: 0.77rem;
    color: var(--text-secondary);
    line-height: 1.55;
  }
</style>
