<script lang="ts">
  /**
   * Bevy Remote (right tool window) — the running game's world as one tree: entities, resources and states.
   * A leaf's detail opens in the bottom panel, where the game-wide views (statistics, screen, input, console)
   * live too. Mounted only while shown, so a hidden sidebar stops asking the game for anything.
   */
  import { onMount } from 'svelte';
  import { Unplug } from 'lucide-svelte';
  import BevyIcon from '../BevyIcon.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import { brpHintsStore } from '$lib/stores/bennu/brp-hints.svelte';
  import { brpDriveStore } from '$lib/stores/bennu/brp-drive.svelte';
  import BrpConnect from './BrpConnect.svelte';
  import BrpDespawnConfirm from './BrpDespawnConfirm.svelte';
  import BrpEndpointMenu from './BrpEndpointMenu.svelte';
  import BrpTree from './BrpTree.svelte';

  let despawning = $state<number | null>(null);

  onMount(() => {
    void brp.init().then(() => brp.resume());
    return () => brp.suspend();
  });

  const live = $derived(brp.status.connected || brp.status.state === 'reconnecting');

  // What the companion plugin offered belongs to one game: a restart (new generation) forgets it.
  $effect(() => {
    const generation = brp.status.generation;
    brpDriveStore.reset();
    if (brp.status.connected) void brpHintsStore.load(generation);
  });
</script>

<PanelShell title="Bevy Remote">
  {#snippet icon()}<BevyIcon size={13} />{/snippet}
  {#snippet actions()}
    {#if live}
      <Badge
        variant="tone"
        size="sm"
        dot
        tone={brp.status.connected ? 'success' : 'warning'}
        label={brp.status.connected ? 'live' : 'reconnecting…'}
      />
      <BrpEndpointMenu />
      <IconButton tooltip="Disconnect" size={22} onclick={() => void brp.disconnect()}><Unplug size={13} /></IconButton>
    {/if}
  {/snippet}

  {#if !live}
    <BrpConnect />
  {:else}
    {#if brp.retry}
      <Alert variant="warning" compact text={`The game stopped answering — reconnecting (attempt ${brp.retry.attempt} of ${brp.retry.max}).`} />
    {/if}
    {#if brp.status.non_loopback}
      <Alert variant="warning" compact text="This endpoint is not on this machine. Edits, spawns and despawns act on that game." />
    {/if}
    <BrpTree onDespawn={(id) => (despawning = id)} />
  {/if}
</PanelShell>

<BrpDespawnConfirm id={despawning} onClose={() => (despawning = null)} />
