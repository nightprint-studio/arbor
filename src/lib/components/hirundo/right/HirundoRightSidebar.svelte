<script lang="ts">
  /**
   * HirundoRightSidebar — the runner, the histories and the schedules, one per icon of the right rail.
   * They sit on the right, beside the collections you pick from on the left, because they are lists that are
   * read *while* you work in the tree and the editor: a run's results next to the requests they are about.
   */
  import { Clock, History, Layers, Play, Send } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { HIRUNDO_RIGHT_SECTIONS } from '../hirundo-palette';
  import HistoryPanel from './HistoryPanel.svelte';
  import RunnerPanel from './RunnerPanel.svelte';
  import SchedulesPanel from './SchedulesPanel.svelte';
  import CookiesPanel from './CookiesPanel.svelte';
  import StreamPanel from './StreamPanel.svelte';

  const ICONS = { runner: Play, history: History, schedules: Clock, cookies: Layers, stream: Send };
  const section = $derived(HIRUNDO_RIGHT_SECTIONS.find((s) => s.id === hirundoUiStore.rightSection) ?? HIRUNDO_RIGHT_SECTIONS[0]);
</script>

<PanelShell title={section.label} scrollable={false}>
  {#snippet icon()}{@const Icon = ICONS[section.id]}<Icon size={14} />{/snippet}
  <div class="rs-body">
    {#if hirundoUiStore.rightSection === 'runner'}<RunnerPanel />
    {:else if hirundoUiStore.rightSection === 'history'}<HistoryPanel />
    {:else if hirundoUiStore.rightSection === 'schedules'}<SchedulesPanel />
    {:else if hirundoUiStore.rightSection === 'cookies'}<CookiesPanel />
    {:else}<StreamPanel />{/if}
  </div>
</PanelShell>

<style>
  .rs-body { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .rs-body > :global(*) { flex: 1; min-height: 0; }
</style>
