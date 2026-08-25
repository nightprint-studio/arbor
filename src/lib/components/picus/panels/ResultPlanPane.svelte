<script lang="ts">
  /**
   * The plan pane's four states: asking, failed, answered, never asked.
   *
   * Beside the rows rather than in a panel of its own because it answers a question
   * asked *about the rows on screen* — "why was that slow" — and a separate panel
   * would put the answer somewhere the question is not.
   */
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import QueryPlanView from './QueryPlanView.svelte';
  import type { TabPlan } from '$lib/stores/picus/plan.svelte';

  interface Props {
    /** `null` for a tab that has never asked for one. */
    planState: TabPlan | null;
  }

  let { planState }: Props = $props();
</script>

{#if planState?.running}
  <StateBlock tone="loading">
    {#snippet spinner()}<Spinner size={14} />{/snippet}
    <span>
      {planState.measuring
        ? 'Running the statement to measure it…'
        : 'Asking the server for the plan…'}
    </span>
  </StateBlock>
{:else if planState?.error}
  <StateBlock tone="error" label={planState.error} />
{:else if planState?.plan}
  <QueryPlanView plan={planState.plan} sql={planState.sql} />
{:else}
  <StateBlock
    tone="info"
    label="Explain shows how the server would run the statement the caret is in. Analyze runs it and reports what actually happened."
  />
{/if}
