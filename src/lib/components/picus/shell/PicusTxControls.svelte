<script lang="ts">
  /**
   * The transaction cluster of the Picus toolbar.
   *
   * ## Why this is loud
   *
   * An open transaction changes the meaning of **every** statement that follows it:
   * the DELETE that ran is not gone from the database, the row the grid shows is not
   * what another session sees, and closing the window throws all of it away. That is
   * not a status worth a discreet grey chip — it is the single fact on this bar that
   * the user must never have to look for. So while a transaction is open the cluster
   * is a coloured band with the two decisions beside it, and while none is open it is
   * one quiet ghost button.
   *
   * ## Failed is its own state, and only one thing is offered in it
   *
   * A statement that fails inside a transaction leaves PostgreSQL in an *aborted*
   * block: nothing else will run on the connection until it ends. Commit is disabled
   * there rather than hidden, with the reason attached — PostgreSQL would accept the
   * COMMIT, perform a rollback, and report success, which is the one outcome worse
   * than a refusal.
   *
   * ## The state is the server's
   *
   * Nothing here is inferred from which button was last pressed. `txStore.refresh`
   * asks the engine after every statement (`busy` falling is the signal), which is
   * what makes a transaction the user opened by typing `BEGIN` in the editor — or
   * aborted by getting a statement wrong — show up here at all.
   */
  import { Check, Layers, TriangleAlert, Undo2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { txStore } from '$lib/stores/picus/tx.svelte';
  import type { Dialect } from '$lib/types/picus';

  interface Props {
    /** The connection the active tab runs against. Empty when the tab has none. */
    connectionId: string;
    /** Its engine — decides whether any of this is offered at all. */
    dialect: Dialect | undefined;
    /** A statement is in flight on this tab. Its falling edge re-reads the state. */
    busy?: boolean;
    /** Whether the connection has a session. Everything here is a statement sent on
     *  one, so with no session there is nothing any of these buttons can do. */
    sessionOpen?: boolean;
  }

  let { connectionId, dialect, busy = false, sessionOpen = true }: Props = $props();

  /**
   * Why none of this can be pressed, or `''` when it can.
   *
   * `BEGIN`, `COMMIT` and `ROLLBACK` are statements like any other: they need an open
   * session. Offering them on a closed connection is offering a round trip whose only
   * possible outcome is the backend's refusal — and the state this cluster draws is
   * the *server's*, so on a connection that stopped answering the band itself may be
   * the last thing it said rather than what is true now.
   */
  const blocked = $derived(sessionOpen ? '' : 'The connection is not open — connect it first.');

  const ROLLBACK_ONLY =
    'A statement failed inside this transaction. PostgreSQL accepts nothing further on '
    + 'this connection until it is rolled back.';

  const COMMIT_REFUSED =
    'This transaction cannot be committed: the engine would discard every statement in it '
    + 'and still report success. Rolling back is the only honest way out.';

  const capability = $derived(txStore.capability(dialect));
  const supported = $derived(!!capability?.supported);
  const snapshot = $derived(txStore.snapshot(connectionId));
  const open = $derived(snapshot.state !== 'none');
  const failed = $derived(snapshot.state === 'failed');

  /**
   * What a rollback would actually undo here — the one capability the user has to
   * know *before* they rely on it. PostgreSQL's DDL is inside the transaction;
   * Oracle's is not, and an install script that adds a column and then fails leaves
   * the column behind.
   */
  const ddlNote = $derived(
    capability?.transactionalDdl
      ? 'Schema changes are inside this transaction — a rollback undoes them too.'
      : 'Schema changes are NOT inside this transaction on this engine: a rollback will not undo them.',
  );

  // `picus-be` may not be up when this first mounts, which costs nothing: the
  // controls stay hidden and the next mount asks again.
  $effect(() => {
    void txStore.ensureCapabilities();
  });

  // The first state read — and a re-read when the tab is rebound to another
  // database. Gated on `supported` so an engine without transactions is never asked
  // a question it has no answer to; it re-runs by itself once the capabilities land.
  $effect(() => {
    const id = connectionId;
    if (id && supported) void txStore.refresh(id);
  });

  // Re-read after every statement: the statement is what opens a transaction when
  // the user types `BEGIN`, and what fails one when it goes wrong.
  //
  // Plain `let`, not `$state`: it holds the previous value of a prop, and making it
  // reactive would put this effect's own write into its dependency set.
  let ranBefore = false;
  $effect(() => {
    const running = busy;
    const id = connectionId;
    if (ranBefore && !running && id && supported) void txStore.refresh(id);
    ranBefore = running;
  });

  async function act(verb: 'begin' | 'commit' | 'rollback') {
    if (!connectionId) return;
    const error = await txStore[verb](connectionId);
    if (error) {
      toastStore.show(error, 'error');
      return;
    }
    // Begin says nothing: the band appearing is the confirmation, and a toast on top
    // of it is one more thing to dismiss. Ending a transaction is worth confirming in
    // words — it is the moment the work became permanent, or stopped existing.
    const said = txStore.snapshot(connectionId).message;
    if (verb !== 'begin' && said) toastStore.show(said, 'success');
  }
</script>

{#if supported && connectionId}
  {#if open}
    <span class="tx-band" class:failed use:tooltip={failed ? ROLLBACK_ONLY : ddlNote}>
      {#if failed}<TriangleAlert size={12} />{:else}<Layers size={12} />{/if}
      <span class="tx-label">{failed ? 'Transaction failed' : 'Transaction open'}</span>
      <span class="tx-why">{failed ? 'nothing will run until it is rolled back' : 'uncommitted'}</span>
    </span>
    <Button
      variant="ghost"
      size="sm"
      disabled={!!blocked || failed || snapshot.busy}
      tooltip={blocked || (failed ? COMMIT_REFUSED : 'Commit — make every statement in this transaction permanent')}
      ariaLabel="Commit transaction"
      onclick={() => void act('commit')}
    >
      {#snippet iconStart()}<Check size={13} />{/snippet}
      Commit
    </Button>
    <Button
      variant="ghost"
      size="sm"
      color="var(--warning)"
      disabled={!!blocked || snapshot.busy}
      tooltip={blocked || 'Roll back — undo every statement in this transaction'}
      ariaLabel="Roll back transaction"
      onclick={() => void act('rollback')}
    >
      {#snippet iconStart()}<Undo2 size={13} />{/snippet}
      Rollback
    </Button>
  {:else}
    <Button
      variant="icon"
      size="sm"
      disabled={!!blocked || snapshot.busy}
      tooltip={blocked || 'Begin a transaction — statements stop taking effect until you commit'}
      ariaLabel="Begin transaction"
      onclick={() => void act('begin')}
    >
      {#snippet iconStart()}<Layers size={14} />{/snippet}
    </Button>
  {/if}
{/if}

<style>
  /* A band, not a chip. It carries the one fact on this bar that must never be
     hunted for, so it takes the warning ramp and a border rather than muted text —
     and the failed form escalates to the error ramp, because the two states differ
     in what can still be done, not merely in degree. */
  .tx-band {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px;
    margin-left: 4px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-sm);
    background: var(--warning-subtle);
    color: var(--warning);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-2xs);
    white-space: nowrap;
  }
  .tx-band.failed {
    border-color: var(--error);
    background: var(--error-subtle);
    color: var(--error);
  }
  .tx-label {
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }
  /* The consequence, in lower case beside the label: what is at stake, not a second
     shout. */
  .tx-why { opacity: 0.85; }
</style>
