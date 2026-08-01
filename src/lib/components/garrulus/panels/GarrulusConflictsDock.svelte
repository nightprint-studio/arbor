<script lang="ts">
  /**
   * The bottom dock: tasks, problems and conflicts, behind one tab strip.
   *
   * This exists so the shell mounts *one* thing behind *one* flag. The dock's
   * geometry (which edge, how tall, how it slides) and the wiring a resolution
   * needs (re-probe the sync state, so the title bar's control stops saying
   * "2 conflicts to resolve" the moment the second one is settled) belong with the
   * panels they serve, not scattered through the window's layout.
   *
   * **The strip is built from `DOCK_PANELS`, not from a list written here.** The
   * palette addresses these sections by id and the ui store validates the same
   * ids; a fourth list in this file is a fourth place for them to drift. Filtering
   * the palette's labelled tabs by what the store actually has also means a tab
   * whose panel does not exist yet — `history` today — is absent rather than
   * present and inert, which is the doctrine the palette states about itself: a
   * command that lands nowhere is worse than one that is not listed. When history
   * gains a panel and a `DockPanel` id, it appears here with no edit.
   *
   * **A section is mounted the first time it is shown and then kept**, hidden
   * rather than destroyed. Conflicts and Problems would only re-read, which is
   * cheap; the task scan is a vault's worth of reads the user explicitly asked
   * for, and throwing it away because they glanced at another tab would make the
   * panel unusable.
   *
   * Nothing here writes. The only calls that change a file are behind the buttons
   * on a conflict card and on a task's checkbox (§4.2).
   */
  import { untrack } from 'svelte';
  import { ListTodo, RefreshCw } from 'lucide-svelte';
  import PanelCard from '$lib/components/shared/ui/PanelCard.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import ConflictsPanel from './ConflictsPanel.svelte';
  import ProblemsPanel from './ProblemsPanel.svelte';
  import TasksPanel from './TasksPanel.svelte';
  import { GARRULUS_DOCK_TABS, garrulusPaletteIcon } from '../garrulus-palette';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';
  import { garrulusUiStore, DOCK_PANELS, type DockPanel } from '$lib/stores/garrulus/ui.svelte';

  interface Props {
    /** Close the dock — the host owns the flag this panel is mounted behind. */
    onClose: () => void;
    /** Open a note in the editor. Omit while no editor is mounted: the panels
     *  then disable the verb and say why, rather than offering one that goes
     *  nowhere. */
    onOpenNote?: (path: string) => void;
    /** Create a note a `[[link]]` is waiting for — the Problems panel's one
     *  constructive action. Omit until something can create notes. */
    onCreateNote?: (title: string, from: string) => void;
    /** Dock height in px. Session-only UI state, so the host holds it. */
    height?: number;
    onResize?: (px: number) => void;
  }

  let { onClose, onOpenNote, onCreateNote, height = 280, onResize }: Props = $props();

  const active = $derived(garrulusUiStore.dockPanel);

  /** Sections that have been shown at least once, and so stay mounted. */
  let seen = $state<string[]>([garrulusUiStore.dockPanel]);
  $effect(() => {
    const id = garrulusUiStore.dockPanel;
    // `untrack`: the list is written here and read only by the template, so the
    // effect must not take a dependency on what it appends to.
    untrack(() => {
      if (!seen.includes(id)) seen = [...seen, id];
    });
  });

  /** The conflict count the sync state already carries — the same number the
   *  title bar's control shows, rather than a second one that could disagree. */
  const conflictCount = $derived(
    garrulusSyncStore.tag === 'conflict' ? garrulusSyncStore.count : 0,
  );

  const tabs = $derived<TabItem[]>(
    GARRULUS_DOCK_TABS
      .filter((tab) => (DOCK_PANELS as readonly string[]).includes(tab.id))
      .map((tab) => ({
        id: tab.id,
        label: tab.label,
        icon: garrulusPaletteIcon(tab.icon),
        iconSize: 13,
        badge: tab.id === 'conflicts' && conflictCount > 0 ? conflictCount : undefined,
      })),
  );

  let conflictsView = $state<{ reload: () => Promise<void> } | null>(null);
  let problemsView = $state<{ reload: () => Promise<void> } | null>(null);
  let tasksView = $state<{ refresh: () => Promise<void> } | null>(null);
</script>

<PanelCard
  orientation="bottom"
  initialSize={height}
  minSize={160}
  maxSize={560}
  {onResize}
>
  <div class="dock">
    <BottomPanelHeader {onClose}>
      {#snippet children()}
        <div class="dock-tabs">
          <Tabs
            items={tabs}
            value={active}
            variant="panel"
            size="sm"
            ariaLabel="Bottom dock sections"
            onSelect={(id) => garrulusUiStore.showDock(id as DockPanel)}
          />
        </div>
      {/snippet}

      {#snippet actions()}
        {#if active === 'conflicts'}
          <button
            class="ps-btn"
            type="button"
            use:tooltip={'Re-read the conflict list. Reads only — it settles nothing.'}
            aria-label="Re-read the conflict list"
            onclick={() => void conflictsView?.reload()}
          >
            <RefreshCw size={13} />
          </button>
        {:else if active === 'problems'}
          <button
            class="ps-btn"
            type="button"
            use:tooltip={'Re-read the link graph'}
            aria-label="Re-read the link graph"
            onclick={() => void problemsView?.reload()}
          >
            <RefreshCw size={13} />
          </button>
        {:else if active === 'tasks'}
          <button
            class="ps-btn"
            type="button"
            use:tooltip={'Scan the vault again. Reads every note; it changes nothing.'}
            aria-label="Scan the vault for tasks again"
            onclick={() => void tasksView?.refresh()}
          >
            <ListTodo size={13} />
          </button>
        {/if}
      {/snippet}
    </BottomPanelHeader>

    <div class="dock-body">
      {#if seen.includes('tasks')}
        <div class="dock-section" class:hidden={active !== 'tasks'}>
          <TasksPanel bind:this={tasksView} {onOpenNote} />
        </div>
      {/if}
      {#if seen.includes('problems')}
        <div class="dock-section" class:hidden={active !== 'problems'}>
          <ProblemsPanel bind:this={problemsView} {onOpenNote} {onCreateNote} />
        </div>
      {/if}
      {#if seen.includes('conflicts')}
        <div class="dock-section" class:hidden={active !== 'conflicts'}>
          <ConflictsPanel
            bind:this={conflictsView}
            hideHeader
            {onOpenNote}
            onResolved={() => void garrulusSyncStore.refresh()}
          />
        </div>
      {/if}
    </div>
  </div>
</PanelCard>

<style>
  .dock {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    min-height: 0;
    background: var(--bg-base);
    overflow: hidden;
  }

  .dock-tabs {
    display: flex;
    align-items: stretch;
    align-self: stretch;
    margin-left: 6px;
    min-width: 0;
  }
  .dock-tabs :global(.tabs) { flex: 1; min-width: 0; height: 100%; }

  .dock-body { flex: 1; min-height: 0; position: relative; overflow: hidden; }
  .dock-section {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .dock-section.hidden { display: none; }
</style>
