<script lang="ts">
  /**
   * SchedulesPanel — the project's timetable: "every night at 02:00 run Smoke on staging". Each row says
   * when it runs next and how it went last; a switch turns it off without losing it. Schedules fire only
   * while the Hirundo window is open (a run missed in between is skipped or caught up, per schedule).
   */
  import { Pencil, Play, Plus, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { hirundoSchedules } from '$lib/stores/hirundo/schedules.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { hirundoCommands } from '../hirundo-commands';
  import { describe, whenText } from '../schedule-model';

  $effect(() => {
    void hirundoStore.project;
    void hirundoSchedules.load();
  });

  const tone = (r: string) => (r === 'ok' ? 'success' : r === 'failed' ? 'error' : 'neutral');
</script>

<div class="sp">
  {#if !hirundoStore.project}
    <EmptyState message="No project open" />
  {:else}
    <div class="sp-top">
      <Button variant="primary" size="sm" disabled={hirundoStore.tree.length === 0} onclick={() => hirundoCommands.schedule(null)}>{#snippet iconStart()}<Plus size={12} />{/snippet}New schedule</Button>
      <span class="sp-note">Schedules run while this window is open.</span>
    </div>
    {#if hirundoSchedules.items.length === 0}
      <EmptyState message="No schedules" description={hirundoStore.tree.length ? 'Run a collection on a timetable: every night, on weekdays, every hour.' : 'Create a collection first.'} compact />
    {:else}
      <div class="sp-list">
        <RowList items={hirundoSchedules.items} key={(s) => s.id} dimmed={(s) => !s.enabled}>
          {#snippet row(s)}
            <span class="rl-keep"><Toggle checked={s.enabled} ariaLabel={`Enable ${s.name}`} onchange={() => void hirundoSchedules.toggle(s)} /></span>
            <div class="sp-main">
              <b>{s.name}</b>
              <span class="sp-sub">{s.target} · {s.env_id ? (hirundoStore.envs.environments.find((e) => e.id === s.env_id)?.name ?? s.env_id) : 'active environment'} · {describe(s.recurrence)}</span>
            </div>
            <span class="sp-next">{s.enabled ? `next ${whenText(s.next_run_ms)}` : 'off'}</span>
            {#if s.last_result}<Badge variant="status" size="sm" tone={tone(s.last_result)} label={s.last_result} tooltip={s.last_summary} />{/if}
            <span class="rl-keep sp-acts">
              <IconButton tooltip="Run now" size={22} onclick={() => void hirundoSchedules.runNow(s)}><Play size={12} /></IconButton>
              <IconButton tooltip="Edit" size={22} onclick={() => hirundoCommands.schedule(s.id)}><Pencil size={12} /></IconButton>
              <IconButton tooltip="Delete" size={22} variant="danger" onclick={() => void hirundoSchedules.remove(s.id)}><Trash2 size={12} /></IconButton>
            </span>
          {/snippet}
        </RowList>
      </div>
    {/if}
  {/if}
</div>

<style>
  .sp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .sp-top { display: flex; align-items: center; gap: 10px; padding: 8px 12px; }
  .sp-note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .sp-list { padding: 0 12px 8px; overflow: auto; flex: 1; min-height: 0; }
  .sp-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .sp-sub { font-size: var(--font-size-xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sp-next { font-size: var(--font-size-xs); color: var(--text-secondary); white-space: nowrap; }
  .sp-acts { display: flex; gap: 2px; }
</style>
