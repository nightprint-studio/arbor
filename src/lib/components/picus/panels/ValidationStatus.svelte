<script lang="ts">
  /**
   * The live-validation indicator for the query toolbar.
   *
   * One glance says whether the database has accepted what is in the editor: a
   * spinner while a check is in flight, a tick when the last one passed, a count when
   * it did not, and a muted dot when there is nothing to check against (no connection,
   * or an engine that cannot prepare). Nothing at all before anything has been typed.
   *
   * It reads the shared `validationStore`, which follows the active editor — so this
   * takes no props and needs no tab: whatever is on screen is what it reports on.
   */
  import { Check, CircleSlash, TriangleAlert } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { validationStore } from '$lib/stores/picus/validation.svelte';

  const status = $derived(validationStore.status);
  const count = $derived(validationStore.count);
</script>

{#if status === 'checking'}
  <span class="pvs" use:tooltip={'Checking the statements against the database…'}>
    <Spinner size={11} />
  </span>
{:else if status === 'ok'}
  <span class="pvs ok" use:tooltip={'The database accepts every statement here'}>
    <Check size={13} />
  </span>
{:else if status === 'errors'}
  <span
    class="pvs err"
    use:tooltip={`${count} statement${count === 1 ? '' : 's'} the database rejected`}
  >
    <TriangleAlert size={12} />
    {count}
  </span>
{:else if status === 'unavailable'}
  <span class="pvs muted" use:tooltip={'No connection to validate against'}>
    <CircleSlash size={12} />
  </span>
{/if}

<style>
  .pvs {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    line-height: 1;
    padding: 0 4px;
    color: var(--text-secondary);
  }
  .pvs.ok {
    color: var(--success);
  }
  .pvs.err {
    color: var(--error);
    font-variant-numeric: tabular-nums;
  }
  .pvs.muted {
    color: var(--text-disabled);
  }
</style>
