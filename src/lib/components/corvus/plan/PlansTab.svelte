<script lang="ts">
  /**
   * The "Plans" tab of the Pipelines panel. Plans cross repositories and workspaces, so the list
   * does not depend on the repo that happens to be open. Grouped by what needs you first.
   */
  import { Eye, Plus } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import type { Plan } from '$lib/types/corvus/plan';
  import { plansStore } from '$lib/stores/corvus/plans.svelte';
  import PlanRow from './PlanRow.svelte';

  const byRecent = (a: Plan, b: Plan) => b.updated_ms - a.updated_ms;
  const live = $derived(plansStore.plans.filter(p => !p.template));

  const groups = $derived<{ id: string; label: string; plans: Plan[] }[]>([
    { id: 'running', label: 'Running', plans: live.filter(p => p.status === 'running').sort(byRecent) },
    { id: 'attention', label: 'Needs you', plans: live.filter(p => p.status === 'paused').sort(byRecent) },
    { id: 'drafts', label: 'Drafts', plans: live.filter(p => p.status === 'draft').sort(byRecent) },
    { id: 'finished', label: 'Finished', plans: live.filter(p => ['done', 'failed', 'cancelled'].includes(p.status)).sort(byRecent) },
    { id: 'templates', label: 'Templates', plans: plansStore.plans.filter(p => p.template).sort(byRecent) },
  ].filter(g => g.plans.length > 0));

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

  {#if groups.length === 0}
    <EmptyState
      message="No plans yet"
      description="Add a push, a merge request or a commit to a plan from their buttons, or start an empty one."
    />
  {:else}
    <div class="list">
      {#each groups as group (group.id)}
        <div class="group">{group.label} <span class="count">{group.plans.length}</span></div>
        {#each group.plans as plan (plan.id)}
          <PlanRow
            {plan}
            onOpen={() => plansStore.openModal(plan.id)}
            onRun={() => plansStore.run(plan.id)}
            onStop={() => plansStore.stop(plan.id)}
            onDelete={() => plansStore.requestDelete(plan)}
          />
        {/each}
      {/each}
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
  .list { display: flex; flex-direction: column; gap: 6px; padding: 4px 10px 12px; overflow: auto; }
  .group { padding: 10px 4px 2px; font-size: 10.5px; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-muted); }
  .count { margin-left: 4px; opacity: 0.7; }
</style>
