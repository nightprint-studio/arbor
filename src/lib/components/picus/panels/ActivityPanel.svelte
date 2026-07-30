<script lang="ts">
  /**
   * The session monitor — what every backend on this server is doing.
   *
   * It exists because of a failure that actually happened: a statement stopped
   * responding, Cancel did nothing, and from inside Picus there was no way to tell
   * whether the backend was still running, waiting on a lock, or already gone.
   *
   * Three things it is arranged around:
   *
   * * **Blocked is not a flag, it is a chain.** A row that says "blocked" tells you
   *   nothing you can act on. The chain to the session at the root of the wait is
   *   the answer, and that session — the one with nothing in front of it — is the
   *   only one where acting releases anybody. It is marked, and it is the only
   *   place the panel offers an opinion.
   * * **It polls only while it is on screen.** The panel mounts when its dock tab
   *   is showing and nowhere else, so `start` on mount and `stop` on unmount is the
   *   whole visibility rule. A timer running behind a closed dock is a query
   *   against a production server every three seconds, for nobody.
   * * **Selection is the handle.** Cancel and Terminate act on the selected row, so
   *   ↑/↓ and a button reachable by Tab is the entire flow — no pointer anywhere.
   */
  import { untrack } from 'svelte';
  import { Activity, Ban, Power, RefreshCw } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import DataGrid, {
    type DataGridColumn,
    type DataGridValue,
  } from '$lib/components/shared/ui/DataGrid.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import ActivitySessionCell from './ActivitySessionCell.svelte';
  import { stopConfirmation } from './activity-stop-text';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { activityStore, type ActivityRow } from '$lib/stores/picus/activity.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import type { StopKind } from '$lib/ipc/picus/activity';

  /**
   * Poll for exactly as long as this component is alive, and no longer.
   *
   * `untrack` around the call is load-bearing: `start` reads the store's own
   * `connectionId`, which each poll then writes. Tracked, that is an effect which
   * re-arms its own timer every three seconds forever.
   */
  $effect(() => {
    const id = connectionsStore.activeId;
    untrack(() => activityStore.start(id));
    return () => activityStore.stop();
  });

  const rows = $derived(activityStore.rows);

  /** The row the actions apply to; `null` until something is selected. */
  let selectedRow = $state<number | null>(null);
  const selected = $derived<ActivityRow | null>(
    selectedRow === null ? null : rows[selectedRow] ?? null,
  );

  const columns: DataGridColumn[] = [
    { id: 'pid', label: 'PID', type: 'number', width: 84 },
    { id: 'user', label: 'User', width: 110 },
    { id: 'application', label: 'Application', width: 140 },
    { id: 'database', label: 'Database', width: 110 },
    { id: 'state', label: 'State', width: 150 },
    { id: 'queryAge', label: 'Query age', type: 'number', width: 96 },
    { id: 'stateAge', label: 'In state', type: 'number', width: 96 },
    { id: 'txAge', label: 'Txn age', type: 'number', width: 96 },
    { id: 'blocked', label: 'Waiting for', width: 190 },
    { id: 'query', label: 'Statement', width: 520 },
  ];

  /**
   * Ages travel as **numbers** and are formatted on the way to the screen.
   *
   * Putting the formatted string in the row would sort `900ms` after `2m 10s` —
   * on the one column people sort this table by, which is worse than not offering
   * the sort at all.
   */
  const gridRows = $derived<DataGridValue[][]>(
    rows.map((r) => [
      r.session.pid,
      r.session.user,
      r.session.application,
      r.session.database,
      r.session.state,
      r.session.queryAgeMs,
      r.session.stateAgeMs,
      r.session.transactionAgeMs,
      r.session.blockedBy.join(', '),
      r.session.query,
    ]),
  );

  /** The stop being confirmed, or `null`. Both verbs go through the same dialog;
   *  what differs is what it says will happen. */
  let pending = $state<{ row: ActivityRow; kind: StopKind } | null>(null);
  let busy = $state(false);

  const confirmText = $derived(
    pending ? stopConfirmation(pending.row, pending.kind) : { title: '', message: '', detail: '' },
  );

  function ask(kind: StopKind) {
    if (selected) pending = { row: selected, kind };
  }

  async function confirm() {
    if (!pending) return;
    busy = true;
    const { row, kind } = pending;
    const outcome = await activityStore.stopBackend(row.session.pid, kind);
    busy = false;
    pending = null;

    if (!outcome.ok) {
      // The server's own words, verbatim: "permission denied to terminate process"
      // is a different problem from anything Picus could paraphrase it into, and
      // the paraphrase is what would send someone looking in the wrong place.
      toastStore.show(outcome.reason, 'error');
      return;
    }
    if (!outcome.found) {
      // Ordinary rather than a failure: the list is three seconds old by design.
      toastStore.show(`No backend with pid ${row.session.pid} — it had already ended.`, 'info');
      return;
    }
    toastStore.show(
      kind === 'cancel'
        ? `Cancel sent to pid ${row.session.pid}.`
        : `Session ${row.session.pid} terminated.`,
      'success',
    );
  }
</script>

<div class="ap">
  <BottomPanelHeader
    title="Sessions"
    count={rows.length}
    onClose={() => picusUiStore.closeBottom()}
  >
    {#snippet icon()}<Activity size={13} />{/snippet}

    {#if activityStore.blockedCount}
      <Badge
        variant="tone"
        tone="warning"
        size="sm"
        label={`${activityStore.blockedCount} blocked`}
      />
    {/if}
    {#if activityStore.readAt}
      <!-- The SERVER's clock, as the server prints it. Nothing here subtracts it
           from the browser's — the two disagree, and a duration built from both is
           wrong in the way nobody notices. -->
      <span class="ap-read" use:tooltip={'Read from the server at this time, by the server’s clock'}>
        {activityStore.readAt}
      </span>
    {/if}

    {#snippet actions()}
      <Button
        variant="icon"
        size="xs"
        tooltip="Cancel the selected session’s statement — the connection stays open"
        ariaLabel="Cancel the selected statement"
        disabled={!selected}
        onclick={() => ask('cancel')}
      >
        {#snippet iconStart()}<Ban size={13} />{/snippet}
      </Button>
      <Button
        variant="icon"
        size="xs"
        tooltip="Terminate the selected session — its transaction is rolled back"
        ariaLabel="Terminate the selected session"
        disabled={!selected}
        onclick={() => ask('terminate')}
      >
        {#snippet iconStart()}<Power size={13} />{/snippet}
      </Button>
      <Button
        variant="icon"
        size="xs"
        tooltip="Read the server again now"
        ariaLabel="Refresh the session list"
        onclick={() => void activityStore.refreshNow()}
      >
        {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
      </Button>
    {/snippet}
  </BottomPanelHeader>

  <div class="ap-body">
    {#if !activityStore.supported}
      <StateBlock
        tone="info"
        fill={false}
        label="This engine cannot be asked what its sessions are doing."
      />
    {:else if activityStore.error}
      <StateBlock tone="error" label={activityStore.error} />
    {:else if activityStore.loading && !rows.length}
      <!-- Only the FIRST read gets a block. A refresh keeps the table on screen:
           blanking a list somebody is reading, every three seconds, is worse than
           rows that are three seconds old. -->
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the server…</span>
      </StateBlock>
    {:else}
      <DataGrid
        {columns}
        rows={gridRows}
        bind:selectedRow
        showRowNumbers={false}
        filterable
        emptyMessage="No session is connected to this server."
        ariaLabel="Server sessions"
      >
        {#snippet cell({ value, column, rowIndex })}
          <!-- The grid addresses rows by their index in `rows`, never by their
               position on screen, so this stays right under a sort or a filter. -->
          <ActivitySessionCell columnId={column.id} {value} row={rows[rowIndex]} />
        {/snippet}
      </DataGrid>
    {/if}
  </div>
</div>

{#if pending}
  <ConfirmModal
    title={confirmText.title}
    message={confirmText.message}
    detail={confirmText.detail}
    variant={pending.kind === 'terminate' ? 'danger' : 'warning'}
    confirmLabel={pending.kind === 'terminate' ? 'Terminate' : 'Cancel statement'}
    cancelLabel="Leave it alone"
    {busy}
    onConfirm={() => void confirm()}
    onCancel={() => (pending = null)}
  />
{/if}

<style>
  .ap { display: flex; flex-direction: column; flex: 1; min-height: 0; min-width: 0; height: 100%; }
  .ap-body { flex: 1; min-height: 0; display: flex; overflow: hidden; }
  .ap-body > :global(*) { flex: 1; min-width: 0; min-height: 0; }

  .ap-read {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    cursor: help;
    white-space: nowrap;
  }
</style>
