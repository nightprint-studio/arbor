<script lang="ts">
  /**
   * Hosts the Create-MR modal in plan mode. Whoever needs an MR in a plan (the commit menu, the
   * plan board) opens a draft on the store; the form is the same one that creates an MR for real,
   * only its primary action adds a step.
   */
  import CreateMrModal from '$lib/components/corvus/mr/CreateMrModal.svelte';
  import type { CreateMrParams } from '$lib/types/corvus/mr';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';

  const draft = $derived(plansStore.mrDraft);

  function finish(added: boolean) {
    const done = draft?.onDone;
    plansStore.closeMrDraft();
    done?.(added);
  }

  async function add(params: CreateMrParams) {
    if (!draft) return;
    const plan = await plansStore.addMergeRequest(draft, params);
    // `onClose` follows this call, so report the outcome through the flag it will read.
    added = plan !== null;
  }

  let added = false;
</script>

{#if draft}
  {#key draft}
    <CreateMrModal
      planMode
      tabId={draft.tabId}
      currentBranch={draft.source ?? ''}
      onPlan={add}
      onCreated={() => {}}
      onClose={() => { const ok = added; added = false; finish(ok); }}
    />
  {/key}
{/if}
