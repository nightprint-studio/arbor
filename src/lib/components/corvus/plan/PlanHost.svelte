<script lang="ts">
  /** Mounted once in the Corvus shell: the plan modal and the plan-mode Create-MR modal. */
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import PlanMrHost from './PlanMrHost.svelte';
  import PlanModal from './PlanModal.svelte';
</script>

{#if plansStore.modalPlanId !== null}
  <PlanModal />
{/if}
<PlanMrHost />
