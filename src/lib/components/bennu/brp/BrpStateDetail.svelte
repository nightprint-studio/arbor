<script lang="ts">
  /**
   * One Bevy state machine of the game: the variant it is in, and the others it can be moved to. A move
   * writes `NextState<T>` (the game applies it on its next state-transition run); one whose name says it ends
   * the session — a menu, a quit, a reset — asks first.
   */
  import { RefreshCw } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import RowList from '$lib/components/shared/ui/RowList.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import BrpIcon from './icons/BrpIcon.svelte';
  import { isDestructiveTransition } from './brp-state';
  import { shortTypeName } from './brp-value';

  let confirming = $state<string | null>(null);

  const entry = $derived(brp.states.find((s) => s.enumPath === brp.selectedState));
  const canSet = $derived(!!entry?.nextPath && brp.status.can.insert_resource);

  function move(variant: string) {
    if (isDestructiveTransition(variant)) confirming = variant;
    else void brp.setState(variant);
  }

  function confirmed() {
    const variant = confirming;
    confirming = null;
    if (variant) void brp.setState(variant);
  }
</script>

<PaneHeader>
  {#snippet children()}
    <BrpIcon name="state" size={14} />
    {entry ? shortTypeName(entry.enumPath) : 'State'}
  {/snippet}
  {#snippet end()}
    {#if brp.stateCurrent}<Badge variant="tone" tone="success" size="sm" dot label={brp.stateCurrent} />{/if}
    <IconButton tooltip="Refresh" size={22} onclick={() => brp.selectedState && void brp.selectState(brp.selectedState)}><RefreshCw size={13} /></IconButton>
  {/snippet}
</PaneHeader>

{#if !entry}
  <EmptyState message="Select a state" description="Bevy states are the game's modes: menu, playing, paused." />
{:else}
  <div class="sd">
    {#if brp.stateError}
      <Alert variant="warning" compact text={brp.stateError} />
    {:else if !canSet}
      <Alert variant="info" compact text="The game did not register NextState for this state, so it can be read but not changed from here." />
    {/if}
    <RowList items={brp.stateVariants} key={(v) => v.name}>
      {#snippet row(v)}
        <span class="sd-name" class:now={v.name === brp.stateCurrent}>{v.name}</span>
        {#if v.name === brp.stateCurrent}
          <span class="sd-here">current</span>
        {:else if canSet}
          <Button size="sm" variant="ghost" onclick={() => move(v.name)}>Switch to</Button>
        {/if}
      {/snippet}
    </RowList>
  </div>
{/if}

{#if confirming}
  <ConfirmModal
    title="Change the game's state"
    message={`Move ${entry ? shortTypeName(entry.enumPath) : 'the state'} to ${confirming}?`}
    detail="This usually leaves the current session: the game may drop what it has loaded. It cannot be undone from here."
    variant="danger"
    confirmLabel="Switch"
    onConfirm={confirmed}
    onCancel={() => (confirming = null)}
  />
{/if}

<style>
  .sd { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 10px 8px; display: flex; flex-direction: column; gap: 6px; }
  .sd-name { flex: 1; font-size: var(--font-size-sm); color: var(--text-primary); }
  .sd-name.now { font-weight: 600; color: var(--success); }
  .sd-here { font-size: var(--font-size-2xs); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
</style>
