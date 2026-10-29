<script lang="ts">
  /**
   * The Bevy Remote inspector — a bottom tool window onto a running game.
   *
   * Entities | Resources | Types, each a list beside the thing it selects. Everything on screen is
   * composed from the shared widgets, so a plugin could draw the same screens.
   *
   * Mounted only while shown (like Forms): leaving the panel stops the refresh loop and the entity
   * watch, and coming back re-subscribes — a hidden inspector must cost the game nothing.
   */
  import { onMount } from 'svelte';
  import { Unplug } from 'lucide-svelte';
  import BevyIcon from '../BevyIcon.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import ColumnPanes from '$lib/components/shared/ui/ColumnPanes.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuBrpStore as brp, type BrpTab } from '$lib/stores/bennu/brp.svelte';
  import type { BrpTypeInfo, BrpTypeKind } from '$lib/ipc/bennu/brp';
  import BrpAddModal from './BrpAddModal.svelte';
  import BrpConnect from './BrpConnect.svelte';
  import BrpEntityList from './BrpEntityList.svelte';
  import BrpInspector from './BrpInspector.svelte';
  import BrpResources from './BrpResources.svelte';
  import BrpTypes from './BrpTypes.svelte';
  import { entityTag } from './brp-value';

  const TABS = [
    { id: 'entities', label: 'Entities' },
    { id: 'resources', label: 'Resources' },
    { id: 'types', label: 'Types' },
  ];

  let adding = $state<{ target: 'component' | 'resource'; initial?: BrpTypeInfo } | null>(null);
  let despawning = $state<number | null>(null);

  onMount(() => {
    void brp.init().then(() => brp.resume());
    return () => brp.suspend();
  });

  const live = $derived(brp.status.connected || brp.status.state === 'reconnecting');

  function addToSelection(type: BrpTypeInfo, kind: BrpTypeKind) {
    adding = { target: kind, initial: type };
  }

  async function confirmDespawn() {
    const id = despawning;
    despawning = null;
    if (id !== null) await brp.despawn(id);
  }
</script>

<div class="bp">
  <BottomPanelHeader title="Bevy Remote" onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<BevyIcon size={13} />{/snippet}
    {#if live}
      <Tabs items={TABS} value={brp.tab} variant="pill" size="sm" onSelect={(id) => brp.setTab(id as BrpTab)} />
      <Badge
        variant="tone"
        size="sm"
        dot
        tone={brp.status.connected ? 'success' : 'warning'}
        label={brp.status.connected ? (brp.status.endpoint ?? 'connected') : 'reconnecting…'}
      />
    {/if}
    {#snippet actions()}
      {#if live}
        <IconButton tooltip="Disconnect" size={22} onclick={() => void brp.disconnect()}>
          <Unplug size={13} />
        </IconButton>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  {#if !live}
    <BrpConnect />
  {:else}
    {#if brp.retry}
      <Alert variant="warning" compact text={`The game stopped answering — reconnecting (attempt ${brp.retry.attempt} of ${brp.retry.max}).`} />
    {/if}
    {#if brp.status.non_loopback}
      <Alert variant="warning" compact text="This endpoint is not on this machine. Edits, spawns and despawns act on that game." />
    {/if}
    <div class="bp-body">
      {#if brp.tab === 'entities'}
        <ColumnPanes leftWidth="360px">
          {#snippet left()}<BrpEntityList onDespawn={(id) => (despawning = id)} />{/snippet}
          {#snippet right()}
            <BrpInspector onAdd={() => (adding = { target: 'component' })} onDespawn={(id) => (despawning = id)} />
          {/snippet}
        </ColumnPanes>
      {:else if brp.tab === 'resources'}
        <ColumnPanes leftWidth="360px">
          {#snippet left()}<BrpResources part="list" onAdd={() => (adding = { target: 'resource' })} />{/snippet}
          {#snippet right()}<BrpResources part="detail" />{/snippet}
        </ColumnPanes>
      {:else}
        <ColumnPanes leftWidth="360px">
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

{#if despawning !== null}
  <ConfirmModal
    title="Despawn entity"
    message={`Despawn ${entityTag(despawning)} from the running game?`}
    detail="Its children go with it. The game is not restarted, and this cannot be undone."
    variant="danger"
    confirmLabel="Despawn"
    onConfirm={() => void confirmDespawn()}
    onCancel={() => (despawning = null)}
  />
{/if}

<style>
  .bp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bp-body { flex: 1; min-height: 0; }
</style>
