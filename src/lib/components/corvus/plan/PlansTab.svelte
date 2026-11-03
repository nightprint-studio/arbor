<script lang="ts">
  /**
   * The "Plans" tab of the Pipelines panel. Plans cross repositories and workspaces, so the list
   * does not depend on the repo that happens to be open.
   */
  import { Eye, Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import PlanRow from './PlanRow.svelte';

  const live = $derived(plansStore.plans.filter(p => !p.template).sort((a, b) => b.updated_ms - a.updated_ms));
  const templates = $derived(plansStore.plans.filter(p => p.template));

  async function create() {
    const plan = await plansStore.create();
    if (plan) plansStore.openModal(plan.id);
  }
</script>

<div class="plans">
  <div class="tools">
    <span class="scope"><Eye size={12} /> All workspaces</span>
    <span class="note">Plans run steps across repositories; they do not depend on the open one.</span>
    <span class="grow"></span>
    <Button size="sm" variant="secondary" onclick={create}><Plus size={13} /> New plan</Button>
  </div>

  {#if live.length === 0 && templates.length === 0}
    <EmptyState
      message="No plans yet"
      description="Add a push, a merge request or a commit to a plan from their buttons, or start an empty one."
    />
  {:else}
    <div class="list">
      {#each live as plan (plan.id)}
        <PlanRow {plan} onOpen={() => plansStore.openModal(plan.id)} />
      {/each}
      {#if templates.length}
        <div class="group">Templates</div>
        {#each templates as plan (plan.id)}
          <PlanRow {plan} onOpen={() => plansStore.openModal(plan.id)} />
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .plans { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .tools { display: flex; align-items: center; gap: 10px; padding: 8px 14px 4px; color: var(--text-muted); font-size: 12px; }
  .scope {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 1px 9px;
    border: 1px solid var(--border-subtle);
    border-radius: 99px;
    background: var(--bg-elevated);
  }
  .grow { flex: 1; }
  .list { display: flex; flex-direction: column; gap: 6px; padding: 6px 10px 12px; overflow: auto; }
  .group { padding: 10px 4px 2px; font-size: 10.5px; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-muted); }
</style>
