<script lang="ts">
  /**
   * The managed processes of the project (the ones the assistant or a recipe started) and the output
   * of the selected one. ↑ ↓ move through the list, Delete stops the selected process.
   */
  import { Square, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import { bennuLiveStore as live } from '$lib/stores/bennu/live.svelte';
  import LiveLogView from './LiveLogView.svelte';
  import { processState, stepIndex, uptimeText } from './live-format';

  const processes = $derived(live.state?.processes ?? []);
  const selected = $derived(processes.find((p) => p.id === live.selectedId) ?? null);
  const lines = $derived(live.logLines.map((l) => l.text));
  const classes = $derived(live.logLines.map((l) => (l.stream === 'stderr' ? 'line-stderr' : l.stream === 'system' ? 'line-marker' : '')));
  const keys = $derived(live.logLines.map((l) => l.seq));
  const finished = (phase: string) => phase === 'exited' || phase === 'stopped';

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Delete' && selected && !finished(selected.phase)) {
      e.preventDefault();
      void live.stop(selected.id);
      return;
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
    e.preventDefault();
    const current = processes.findIndex((p) => p.id === live.selectedId);
    const next = stepIndex(current, processes.length, e.key);
    if (next >= 0 && processes[next]) void live.select(processes[next].id);
  }
</script>

<ColumnPanes leftWidth="340px">
  {#snippet left()}
    <PaneHeader title="Processes" count={processes.length}>
      {#snippet end()}
        <IconButton tooltip="Stop all" size={20} disabled={live.running === 0} onclick={() => void live.stopAll()}>
          <Square size={11} />
        </IconButton>
      {/snippet}
    </PaneHeader>
    {#if processes.length === 0}
      <EmptyState compact message="No managed processes" description="Run a recipe, or ask Imber to start a service." />
    {:else}
      <div class="plist" role="listbox" tabindex="0" aria-label="Managed processes" onkeydown={onKeydown}>
        {#each processes as p (p.id)}
          {@const st = processState(p)}
          <SidebarItem selected={p.id === live.selectedId} onclick={() => void live.select(p.id)}>
            {p.name}
            {#snippet subtitle()}{p.owner ? `Imber ${p.owner} · ` : ''}{p.group ? `${p.group} · ` : ''}{uptimeText(p, Date.now())}{p.ports.length ? ` · :${p.ports.join(', :')}` : ''}{/snippet}
            {#snippet badges()}<Badge variant="tone" size="sm" dot tone={st.tone} label={st.label} />{/snippet}
          </SidebarItem>
        {/each}
      </div>
    {/if}
  {/snippet}
  {#snippet right()}
    {#if selected}
      <PaneHeader loading={selected.phase === 'running'}>
        <span class="cmd" title={selected.command}>$ {selected.command}</span>
        {#snippet end()}
          {#if selected.note}<span class="note">{selected.note}</span>{/if}
          {#if !finished(selected.phase)}
            <IconButton tooltip="Stop (Delete)" size={20} variant="danger" onclick={() => void live.stop(selected.id)}>
              <Trash2 size={11} />
            </IconButton>
          {/if}
        {/snippet}
      </PaneHeader>
      <div class="log"><LiveLogView {lines} {classes} {keys} emptyMessage="No output yet." /></div>
    {:else}
      <EmptyState message="Select a process to read its output" />
    {/if}
  {/snippet}
</ColumnPanes>

<style>
  .plist { overflow-y: auto; flex: 1; min-height: 0; outline: none; }
  .plist:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  .cmd { font-family: var(--font-code); text-transform: none; letter-spacing: 0; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .note { text-transform: none; letter-spacing: 0; color: var(--text-muted); }
  .log { flex: 1; min-height: 0; }
</style>
