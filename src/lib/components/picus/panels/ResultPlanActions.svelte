<script lang="ts">
  /**
   * The two ways of asking for a plan.
   *
   * Both buttons and the confirmation live together because they are one decision:
   * Explain asks the server how it *would* run the statement, Analyze makes it run.
   * Splitting the control from the warning it needs is how a destructive-ish action
   * ends up one refactor away from being fired without one.
   *
   * Rendered into the panel header; what comes back is rendered by
   * `ResultPlanPane`. One file for how you ask, one for what you get.
   */
  import { Gauge, Network } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { queryStore } from '$lib/stores/picus/query.svelte';
  import { picusPlanStore } from '$lib/stores/picus/plan.svelte';
  import type { Connection } from '$lib/types/picus';

  interface Props {
    tabId: string;
    conn: Connection | null;
    /** A request is already in flight, so neither button should start another. */
    busy: boolean;
  }

  let { tabId, conn, busy }: Props = $props();

  /** Ask the server what it *would* do. Nothing is executed. */
  function explain() {
    if (!tabId || !conn) return;
    queryStore.setPane(tabId, 'plan');
    void picusPlanStore.explain(tabId, conn.id, conn.dialect);
  }

  /**
   * Ask the server what it *did* — which means running the statement.
   *
   * Confirmed rather than fired, and the confirmation names the consequence. The
   * backend refuses to measure anything that is not a read, so this can never be a
   * write; it can still be the four-minute report the user was only curious about.
   */
  let confirmMeasure = $state(false);
  function measure() {
    confirmMeasure = false;
    if (!tabId || !conn) return;
    queryStore.setPane(tabId, 'plan');
    void picusPlanStore.measure(tabId, conn.id, conn.dialect);
  }
</script>

<!-- Two buttons, never one with a modifier: the first asks the server what it would
     do, the second makes it do it. That difference is the whole feature, and a flag
     on a single control would hide it. -->
<Button
  variant="icon"
  size="xs"
  tooltip={'Explain — ask the server how it would run this statement. Nothing is executed.'}
  ariaLabel="Explain the statement"
  disabled={busy}
  onclick={explain}
>
  {#snippet iconStart()}<Network size={13} />{/snippet}
</Button>
<Button
  variant="icon"
  size="xs"
  tooltip={'Analyze — RUNS the statement and reports the real times and row counts.'}
  ariaLabel="Analyze the statement (runs it)"
  disabled={busy}
  onclick={() => (confirmMeasure = true)}
>
  {#snippet iconStart()}<Gauge size={13} />{/snippet}
</Button>

{#if confirmMeasure}
  <!-- The consequence, before it happens. Analyze is not a display option of
       Explain: it executes the statement, and on a report that takes minutes the
       difference is the user's afternoon. -->
  <ConfirmModal
    title="Analyze runs the statement"
    message="Measuring a plan means executing the statement and reporting what really happened."
    detail="Only a read can be measured — anything else is refused. A slow statement will take as long as it takes; Cancel on this connection stops it."
    variant="warning"
    confirmLabel="Run and measure"
    onConfirm={measure}
    onCancel={() => (confirmMeasure = false)}
  />
{/if}
