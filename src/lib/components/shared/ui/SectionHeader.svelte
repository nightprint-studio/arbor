<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    description?: string;
    /**
     * Controls belonging to the section as a whole — "Add…", "Clear", a filter.
     *
     * Here rather than left to each consumer because a section's own action is
     * always wanted on the title's baseline, and a caller placing it by hand puts
     * it a few pixels off from the last caller who did the same. The title keeps
     * its baseline whether or not anything is passed.
     */
    actions?: Snippet;
  }

  let { title, description, actions }: Props = $props();
</script>

<div class="section-header">
  <div class="row">
    <h2>{title}</h2>
    {#if actions}
      <div class="actions">{@render actions()}</div>
    {/if}
  </div>
  {#if description}
    <p>{description}</p>
  {/if}
</div>

<style>
  .section-header {
    margin-bottom: 20px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .section-header h2 {
    font-size: var(--font-size-xl);
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
    min-width: 0;
  }
  .section-header p {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }
</style>
