/**
 * Picus explicit transactions — which connections have one open, and what the
 * interface is allowed to offer about it.
 *
 * ## No flag of our own
 *
 * The state here is a **cache of the server's answer**, refreshed after anything
 * that could have changed it, and never inferred. That is the whole design: a
 * boolean set by Begin and cleared by Commit is right until a statement fails inside
 * the transaction, at which point PostgreSQL is in an *aborted* block that refuses
 * everything until it is rolled back — while the flag still says "active" and the
 * toolbar still offers a Commit that would silently throw the work away. So
 * `refresh` is called after every statement, and it asks the engine.
 *
 * ## Per connection, not per tab
 *
 * A transaction belongs to the database session, and a session is one connection.
 * Several query tabs bound to the same connection are inside the *same*
 * transaction whether or not they know it — which is precisely why the indicator
 * has to be loud rather than tucked into the tab that opened it.
 *
 * ## The guard
 *
 * Closing the window, or disconnecting, ends an open transaction by rolling it back.
 * That is not something to discover afterwards, so both paths go through
 * {@link txStore.confirmRelease}, which resolves immediately when nothing is open
 * and otherwise raises a confirmation (rendered by `PicusTxGuard`) naming exactly
 * what would be undone.
 */

import type { Dialect } from '$lib/types/picus';
import type { TxCapability } from '$lib/ipc/picus/db';
import { type TxState, txBegin, txCommit, txRollback, txState } from '$lib/ipc/picus/tx';
import { picusProvidersStore } from './providers.svelte';

/** What is known about one connection's transaction. */
export interface TxSnapshot {
  state: TxState;
  /** The last thing the engine said about it. Shown verbatim; may be empty. */
  message: string;
  /** A transaction verb is in flight on this connection. */
  busy: boolean;
}

const IDLE: TxSnapshot = { state: 'none', message: '', busy: false };

/** Why a confirmation is up — it decides the wording, not the behaviour. */
export type TxGuardScope = 'connection' | 'window';

/** A pending "this would roll back an open transaction" confirmation. */
export interface TxGuardRequest {
  scope: TxGuardScope;
  /** The connections that would be rolled back. Never empty while this is set. */
  ids: string[];
}

/** A transaction is open when it is open — a failed one is still uncommitted work. */
function isOpenState(state: TxState): boolean {
  return state !== 'none';
}

function createTxStore() {
  /** Server-reported state, by connection id. Absent means "never asked". */
  let byConnection = $state<Record<string, TxSnapshot>>({});
  let guard = $state<TxGuardRequest | null>(null);
  /** The rollbacks a confirmed guard is performing. */
  let releasing = $state(false);

  /**
   * Resolver of the promise `confirmRelease` handed its caller.
   *
   * Deliberately not `$state`: it is a continuation, not something anything renders,
   * and a reactive read of it would make every guard render a dependency of the
   * caller that is awaiting it.
   */
  let settle: ((proceed: boolean) => void) | null = null;

  function snapshotOf(id: string): TxSnapshot {
    return byConnection[id] ?? IDLE;
  }

  function put(id: string, patch: Partial<TxSnapshot>) {
    byConnection = { ...byConnection, [id]: { ...snapshotOf(id), ...patch } };
  }

  /** Run one transaction verb and record what the server answered. */
  async function verb(
    id: string,
    call: (id: string) => Promise<{ state: TxState; message: string }>,
  ): Promise<string> {
    if (!id) return '';
    put(id, { busy: true });
    try {
      const outcome = await call(id);
      put(id, { state: outcome.state, message: outcome.message, busy: false });
      return '';
    } catch (e) {
      // The state is now unknown rather than unchanged — a refused Commit leaves the
      // transaction exactly where it was, but a call that failed halfway through does
      // not, and guessing is the one thing this store does not do.
      put(id, { busy: false });
      void refreshOne(id);
      return String(e);
    }
  }

  async function refreshOne(id: string) {
    if (!id) return;
    try {
      const next = await txState(id);
      // The message described the *previous* state. Keeping it across a change would
      // leave "transaction open — nothing takes effect until you commit" sitting
      // beside a band that now says the transaction has failed.
      const message = next === snapshotOf(id).state ? snapshotOf(id).message : '';
      put(id, { state: next, message });
    } catch {
      // A connection that is not open has no transaction, and saying so is more
      // honest than keeping the last thing it said before it went down.
      put(id, { state: 'none', message: '' });
    }
  }

  return {
    /** Every connection currently inside a transaction. */
    get openConnectionIds(): string[] {
      return Object.entries(byConnection)
        .filter(([, snap]) => isOpenState(snap.state))
        .map(([id]) => id);
    },

    /** The confirmation waiting to be answered, or `null`. */
    get guard() { return guard; },
    /** The confirmed guard is rolling back — the dialog's busy state. */
    get releasing() { return releasing; },

    snapshot(id: string | undefined | null): TxSnapshot {
      return id ? snapshotOf(id) : IDLE;
    },

    /** Is this connection inside a transaction — active *or* failed? */
    isOpen(id: string | undefined | null): boolean {
      return !!id && isOpenState(snapshotOf(id).state);
    },

    /**
     * What this engine's transactions cover, or `null` while the descriptors have
     * not been read. `null` is not "unsupported": it is "not asked yet", and the
     * interface must show nothing rather than the wrong thing.
     */
    capability(dialect: Dialect | undefined | null): TxCapability | null {
      return picusProvidersStore.capabilities(dialect)?.transactions ?? null;
    },

    /** Does this engine have explicit transactions at all? */
    supports(dialect: Dialect | undefined | null): boolean {
      return !!this.capability(dialect)?.supported;
    },

    /**
     * Make sure the descriptors are in.
     *
     * A pass-through to the one store that holds them. This used to keep its own
     * copy, its own in-flight promise and its own retry rule — and so did the
     * activity store and the dependency store, which was four readings of a
     * document that describes what the *build* supports and therefore has one
     * answer. Kept as a method rather than deleted because the callers' meaning is
     * "the controls need this before they can decide", which is worth naming.
     */
    ensureCapabilities(): Promise<void> {
      return picusProvidersStore.load();
    },

    /**
     * Re-read one connection's state from the server.
     *
     * Called after every statement, not only after a transaction verb: the statement
     * itself is what opens a transaction when the user types `BEGIN` in the editor,
     * and what fails one when it goes wrong.
     */
    refresh(id: string | undefined | null): Promise<void> {
      return id ? refreshOne(id) : Promise.resolve();
    },

    begin(id: string): Promise<string> { return verb(id, txBegin); },
    commit(id: string): Promise<string> { return verb(id, txCommit); },
    rollback(id: string): Promise<string> { return verb(id, txRollback); },

    /** Drop what is known about a connection — on disconnect, or on forgetting it. */
    forget(id: string) {
      if (!(id in byConnection)) return;
      const { [id]: _gone, ...rest } = byConnection;
      byConnection = rest;
    },

    /**
     * Gate an action that would end whatever transactions these connections hold.
     *
     * Resolves `true` straight away when none of them is inside one — the common
     * case, and it must cost nothing. Otherwise it raises the confirmation and
     * resolves once the user has answered: `true` after the rollbacks have run,
     * `false` when they cancelled and nothing was touched.
     *
     * A second request while one is up is refused rather than queued: two dialogs
     * about the same transactions would be answered once and remembered twice.
     */
    confirmRelease(ids: string[], scope: TxGuardScope): Promise<boolean> {
      const open = ids.filter((id) => isOpenState(snapshotOf(id).state));
      if (!open.length) return Promise.resolve(true);
      if (guard) return Promise.resolve(false);
      guard = { scope, ids: open };
      return new Promise<boolean>((resolve) => {
        settle = resolve;
      });
    },

    /** The dialog's Cancel: nothing was touched, and the caller stops. */
    cancelGuard() {
      const resolve = settle;
      settle = null;
      guard = null;
      resolve?.(false);
    },

    /**
     * The dialog's confirm: roll each transaction back, then let the caller through.
     *
     * The caller is let through **whatever the rollbacks answered**. They are on the
     * way out — closing a window, dropping a connection — and a rollback that failed
     * has not left work to save: the session is about to end, and ending it rolls
     * back anyway. Refusing here would only trap the user in a window they asked to
     * close.
     */
    async confirmGuard() {
      const pending = guard;
      const resolve = settle;
      if (!pending) return;
      releasing = true;
      for (const id of pending.ids) {
        await verb(id, txRollback);
      }
      releasing = false;
      settle = null;
      guard = null;
      resolve?.(true);
    },
  };
}

export const txStore = createTxStore();
