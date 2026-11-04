<script lang="ts">
  /**
   * The Bevy Remote bottom panel — the **detail** of whatever is selected in the sidebar tree (an entity's
   * components, a resource, a state), and the game-wide views one button away: statistics, screen, input,
   * console and the type registry. Everything on screen is composed from the shared widgets, so a plugin could
   * draw the same screens.
   *
   * Mounted only while shown: leaving the panel stops the entity watch, and coming back re-subscribes — a hidden
   * inspector must cost the game nothing.
   */
  import { onMount } from 'svelte';
  import BevyIcon from '../BevyIcon.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuBrpStore as brp, type BrpTab } from '$lib/stores/bennu/brp.svelte';
  import { brpDriveStore } from '$lib/stores/bennu/brp-drive.svelte';
  import { brpHintsStore } from '$lib/stores/bennu/brp-hints.svelte';
  import type { BrpTypeInfo, BrpTypeKind } from '$lib/ipc/bennu/brp';
  import BrpAddModal from './BrpAddModal.svelte';
  import BrpConsole from './BrpConsole.svelte';
  import BrpDespawnConfirm from './BrpDespawnConfirm.svelte';
  import BrpIcon from './icons/BrpIcon.svelte';
  import type { BrpIconName } from './icons/icon-set';
  import BrpInput from './BrpInput.svelte';
  import BrpInspector from './BrpInspector.svelte';
  import BrpResources from './BrpResources.svelte';
  import BrpScreen from './BrpScreen.svelte';
  import BrpStateDetail from './BrpStateDetail.svelte';
  import BrpStats from './BrpStats.svelte';
  import BrpTypes from './BrpTypes.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';

  const VIEWS: { id: BrpTab; label: string; icon: BrpIconName }[] = [
    { id: 'detail', label: 'Detail of the selection', icon: 'detail' },
    { id: 'stats', label: 'Statistics', icon: 'stats' },
    { id: 'screen', label: 'Screen', icon: 'window' },
    { id: 'input', label: 'Input', icon: 'input' },
    { id: 'console', label: 'Console', icon: 'console' },
    { id: 'types', label: 'Type registry', icon: 'types' },
  ];

  let adding = $state<{ target: 'component' | 'resource'; initial?: BrpTypeInfo } | null>(null);
  let despawning = $state<number | null>(null);

  onMount(() => {
    void brp.init().then(() => brp.resume());
    return () => brp.suspend();
  });

  const live = $derived(brp.status.connected || brp.status.state === 'reconnecting');

  $effect(() => {
    const generation = brp.status.generation;
    brpDriveStore.reset();
    if (brp.status.connected) void brpHintsStore.load(generation);
  });

  function addToSelection(type: BrpTypeInfo, kind: BrpTypeKind) {
    adding = { target: kind, initial: type };
  }
</script>

<div class="bp">
  <BottomPanelHeader title="Bevy Remote" onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<BevyIcon size={13} />{/snippet}
    {#if live}
      <div class="bp-views" role="group" aria-label="Views">
        {#each VIEWS as v (v.id)}
          <IconButton tooltip={v.label} size={22} active={brp.tab === v.id} onclick={() => brp.setTab(v.id)}>
            <BrpIcon name={v.icon} size={14} />
          </IconButton>
        {/each}
      </div>
    {/if}
  </BottomPanelHeader>

  {#if !live}
    <EmptyState message="Not connected" description="Open the Bevy Remote sidebar (Ctrl+Shift+A) and connect to the running game." />
  {:else}
    <div class="bp-body">
      {#if brp.tab === 'detail'}
        <!-- Compact on purpose: a few lines of values need a few hundred pixels, not the dock's whole width. -->
        <div class="bp-detail">
          {#if brp.focus === 'entity'}
            <BrpInspector onAdd={() => (adding = { target: 'component' })} onDespawn={(id) => (despawning = id)} />
          {:else if brp.focus === 'resource'}
            <BrpResources onAdd={() => (adding = { target: 'resource' })} />
          {:else if brp.focus === 'state'}
            <BrpStateDetail />
          {:else}
            <EmptyState message="Select something in the sidebar" description="An entity, a resource or a state — its values appear here, editable." compact />
          {/if}
        </div>
      {:else if brp.tab === 'stats'}
        <BrpStats />
      {:else if brp.tab === 'screen'}
        <BrpScreen />
      {:else if brp.tab === 'input'}
        <BrpInput />
      {:else if brp.tab === 'console'}
        <ColumnPanes leftWidth="320px">
          {#snippet left()}<BrpConsole part="list" />{/snippet}
          {#snippet right()}<BrpConsole part="detail" />{/snippet}
        </ColumnPanes>
      {:else}
        <ColumnPanes leftWidth="320px">
          {#snippet left()}<BrpTypes part="list" />{/snippet}
          {#snippet right()}<BrpTypes part="detail" onAddToEntity={addToSelection} />{/snippet}
        </ColumnPanes>
      {/if}
    </div>
  {/if}
</div>

{#if adding}
  <BrpAddModal target={adding.target} initial={adding.initial} onClose={() => (adding = null)} />
{/if}

<BrpDespawnConfirm id={despawning} onClose={() => (despawning = null)} />

<style>
  .bp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bp-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .bp-detail {
    flex: 1; min-height: 0; width: min(100%, 520px); display: flex; flex-direction: column;
    border-right: 1px solid var(--border-subtle);
  }
  .bp-views { display: inline-flex; gap: 2px; margin-left: 6px; }
</style>
