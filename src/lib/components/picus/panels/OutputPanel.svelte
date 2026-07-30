<script lang="ts">
  /**
   * Query history for the active connection — "what did I run on staging".
   *
   * Its own panel, with its own header, since the dock stopped being a tabbed
   * container. Reached from the left rail, beside Consistency: both are about the
   * session and the repository rather than about the document in front of you.
   */
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';

  const history = $derived(
    connectionsStore.activeId ? queryStore.historyFor(connectionsStore.activeId) : [],
  );
</script>

<div class="op">
  <BottomPanelHeader
    title="Output"
    count={history.length}
    onClose={() => picusUiStore.closeBottom()}
  >
    {#if connectionsStore.active}
      <Badge variant="tone" tone="neutral" size="sm" label={connectionsStore.active.name} />
    {/if}
  </BottomPanelHeader>

  <div class="op-body">
    {#if !history.length}
      <StateBlock tone="info" fill={false} label="Nothing has run on this connection yet." />
    {:else}
      {#each history as entry (entry.id)}
        <div class="op-log">
          <span class="op-time">{entry.at}</span>
          <span class="op-sql">{entry.sql.replace(/\s+/g, ' ').slice(0, 140)}</span>
          <!-- `~` where the number was the planner's estimate at the time: a history
               line is read long after the count could have settled it. -->
          <span class="op-meta">
            {entry.approximate ? '~' : ''}{entry.rowCount.toLocaleString()} rows · {entry.elapsedMs} ms
          </span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .op { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; }
  .op-body { flex: 1; min-height: 0; overflow: auto; }

  .op-log {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 3px 12px;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.6;
  }
  .op-log:hover { background: var(--bg-hover); }
  .op-time { color: var(--text-disabled); flex-shrink: 0; }
  .op-sql { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .op-meta { color: var(--text-muted); flex-shrink: 0; }
</style>
