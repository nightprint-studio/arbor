<script lang="ts">
  /**
   * The status mark on a test row.
   *
   * Shape carries the meaning, not just colour: a tick, a cross, a warning triangle and a
   * dash read apart at a glance and keep reading apart for the ~8% of men who cannot tell
   * this palette's green from its red. Colour is the second signal, never the only one.
   *
   * `error` and `failed` are drawn differently on purpose. A failure is a wrong answer — the
   * test ran and disagreed. An error is a broken run — it threw before it could judge
   * anything. Collapsing them into one red mark hides which of the two you are looking at,
   * and they are debugged from opposite ends.
   */
  import { Circle, CircleCheck, CircleMinus, CircleX, TriangleAlert } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import type { RowStatus } from '$lib/stores/bennu/tests.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { status, size = 13 }: { status: RowStatus; size?: number } = $props();

  const MARKS = {
    passed:  { icon: CircleCheck,   label: 'Passed' },
    failed:  { icon: CircleX,       label: 'Failed' },
    error:   { icon: TriangleAlert, label: 'Error' },
    skipped: { icon: CircleMinus,   label: 'Skipped' },
    pending: { icon: Circle,        label: 'Not run' },
  } as const;

  const mark = $derived(status === 'running' ? null : MARKS[status] ?? MARKS.pending);
</script>

{#if mark}
  {@const Ic = mark.icon}
  <span class="tsi tsi-{status}" aria-label={mark.label} use:tooltip={mark.label}><Ic {size} /></span>
{:else}
  <span class="tsi tsi-running" aria-label="Running" use:tooltip={'Running'}><Spinner size={size} /></span>
{/if}

<style>
  .tsi { display: flex; align-items: center; flex-shrink: 0; }
  .tsi-passed { color: var(--success); }
  .tsi-failed, .tsi-error { color: var(--error); }
  .tsi-skipped { color: var(--text-muted); }
  /* Not-run is deliberately the faintest thing on the row: it is the absence of a result,
     and it must not compete with the results next to it. */
  .tsi-pending { color: var(--text-disabled); }
  .tsi-running { color: var(--accent); }
</style>
