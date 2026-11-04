<script lang="ts">
  /** Mounted once in the Corvus shell: the plan modal, the plan-mode Create-MR modal, and the delete confirmation. */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import PlanMrHost from './PlanMrHost.svelte';
  import PlanModal from './PlanModal.svelte';

  const doomed = $derived(plansStore.deleting);
  const stepCount = $derived(doomed ? doomed.phases.reduce((n, p) => n + p.steps.length, doomed.finally.length) : 0);
</script>

{#if plansStore.modalPlanId !== null}
  <PlanModal />
{/if}
<PlanMrHost />

{#if doomed}
  <ConfirmModal
    title={doomed.template ? 'Delete template' : 'Delete plan'}
    message={`Delete “${doomed.name}”?`}
    detail={stepCount > 0 ? `Its ${stepCount} ${stepCount === 1 ? 'step is' : 'steps are'} removed with it. Nothing already run is undone.` : undefined}
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => plansStore.confirmDelete()}
    onCancel={() => plansStore.cancelDelete()}
  />
{/if}
