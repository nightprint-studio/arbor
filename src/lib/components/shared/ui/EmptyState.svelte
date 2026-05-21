<script lang="ts">
  interface Props {
    message: string;
    compact?: boolean;
  }

  let { message, compact = false }: Props = $props();
</script>

<div class="empty" class:compact>
  {message}
</div>

<style>
  .empty {
    padding: 6px 20px;
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    font-style: italic;
  }
  .empty.compact { padding: 4px 16px; }
</style>
