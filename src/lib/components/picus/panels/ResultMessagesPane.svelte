<script lang="ts">
  /**
   * What the statement said, as opposed to what it returned.
   *
   * A pane of the same panel as the rows, never a panel of its own: a failed
   * statement's reason has to be one click from the grid that did not fill, and
   * separating them is how a user ends up staring at an empty grid with the
   * explanation filed somewhere else.
   */
  import type { QueryTabState } from '$lib/stores/picus/query.svelte';

  interface Props {
    messages: QueryTabState['messages'];
  }

  let { messages }: Props = $props();
</script>

<div class="qr-log">
  {#each messages as msg, i (i)}
    <div class="qr-log-line" class:qr-log-error={msg.level === 'error'}>
      <span class="qr-log-time">{msg.time}</span>
      <span>{msg.text}</span>
    </div>
  {:else}
    <p class="qr-log-empty">No message yet.</p>
  {/each}
</div>

<style>
  .qr-log { padding: 6px 0; overflow: auto; width: 100%; }
  .qr-log-line {
    display: flex;
    gap: 10px;
    padding: 1px 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.7;
  }
  .qr-log-time { color: var(--text-disabled); flex-shrink: 0; }
  .qr-log-error { color: var(--error); }
  .qr-log-empty { padding: 8px 12px; font-size: var(--font-size-xs); color: var(--text-disabled); font-style: italic; }
</style>
