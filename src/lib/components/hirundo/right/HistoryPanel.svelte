<script lang="ts">
  /**
   * HistoryPanel — two histories, side by side: what this window *sent* (requests, newest first; a row
   * reopens the request as a new, unsaved tab) and what the runner *ran* (the project's last runs, kept in
   * the profile, each expandable to its requests).
   */
  import { Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { hirundoRunner } from '$lib/stores/hirundo/runner.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { methodColor, methodShort } from '../request-model';
  import type { Method } from '$lib/types/hirundo';

  const time = (at: number) => new Date(at).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  const tone = (s: number | null) => (s === null ? 'neutral' : s >= 500 ? 'error' : s >= 400 ? 'warning' : 'success');
</script>

<div class="hp">
  <section class="hp-col">
    <PaneHeader title="Sent" count={hirundoStore.history.length} />
    {#if hirundoStore.history.length === 0}
      <div class="hp-empty"><EmptyState message="Nothing sent yet" description="Requests sent from this window appear here until it is closed." compact /></div>
    {:else}
      {#each hirundoStore.history as h (h.at)}
        <SidebarItem onclick={() => hirundoStore.openFromHistory(h)}>
          <span class="hp-m" style:color={methodColor(h.method as Method)}>{methodShort(h.method as Method)}</span>
          {h.name || h.url}
          {#snippet badges()}
            <Badge variant="status" size="sm" tone={tone(h.status)} label={h.status === null ? 'failed' : String(h.status)} />
            <span class="hp-t">{new Date(h.at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</span>
          {/snippet}
        </SidebarItem>
      {/each}
    {/if}
  </section>

  <section class="hp-col">
    <PaneHeader title="Runs" count={hirundoRunner.history.length}>
      {#snippet end()}
        {#if hirundoRunner.history.length}<IconButton tooltip="Clear the runs of this project" size={22} variant="danger" onclick={() => void hirundoRunner.clearHistory()}><Trash2 size={12} /></IconButton>{/if}
      {/snippet}
    </PaneHeader>
    {#if hirundoRunner.history.length === 0}
      <div class="hp-empty"><EmptyState message="No runs yet" description="Runs of a collection — by hand or by a schedule — are kept here." compact /></div>
    {:else}
      {#each hirundoRunner.history as run (run.at_ms)}
        {@const bad = run.rows.filter((r) => !r.passed).length}
        <Collapsible chevron>
          {#snippet header()}
            <span class="hp-run">
              <Badge variant="status" size="sm" tone={bad ? 'error' : 'success'} label={bad ? `${bad} ko` : 'ok'} />
              <b>{run.label}</b>
              <span class="hp-t">{run.env_name} · {run.source} · {time(run.at_ms)} · {(run.duration_ms / 1000).toFixed(1)} s</span>
            </span>
          {/snippet}
          <ul class="hp-rows">
            {#each run.rows as r (r.path)}
              <li>
                <Badge variant="status" size="sm" tone={r.passed ? 'success' : 'error'} label={r.passed ? 'PASS' : 'FAIL'} />
                <span class="hp-m" style:color={methodColor(r.method as Method)}>{methodShort(r.method as Method)}</span>
                {r.name}
                <span class="hp-t">{r.status ?? '—'}{r.time_ms != null ? ` · ${r.time_ms} ms` : ''}</span>
                {#each r.problems as p, i (i)}<span class="hp-err">{p}</span>{/each}
              </li>
            {/each}
          </ul>
        </Collapsible>
      {/each}
    {/if}
  </section>
</div>

<style>
  /* Two columns that share the full height — no band of bare background under the shorter list. */
  .hp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .hp-col { flex: 1 1 0; min-height: 0; overflow: auto; padding: 8px 8px 12px; min-width: 0; display: flex; flex-direction: column; }
  .hp-col + .hp-col { border-top: 1px solid var(--border-subtle); }
  .hp-empty { flex: 1; display: grid; place-items: center; }
  .hp-m { font: 600 10px var(--font-mono); margin-right: 6px; }
  .hp-t { font-size: var(--font-size-xs); color: var(--text-muted); }
  .hp-run { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .hp-rows { list-style: none; margin: 0; padding: 2px 0 8px 22px; display: flex; flex-direction: column; gap: 3px; font-size: var(--font-size-sm); }
  .hp-rows li { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .hp-err { flex-basis: 100%; font-size: var(--font-size-xs); color: var(--error); padding-left: 4px; }
</style>
