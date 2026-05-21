<script lang="ts">
  interface Props {
    title: string;
    description?: string;
  }

  let { title, description }: Props = $props();
</script>

<div class="section-header">
  <h2>{title}</h2>
  {#if description}
    <p>{description}</p>
  {/if}
</div>

<style>
  .section-header {
    margin-bottom: 20px;
  }
  .section-header h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
  }
  .section-header p {
    font-size: var(--font-size-sm);
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }
</style>
