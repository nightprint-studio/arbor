/**
 * Picus explicit transactions — `BEGIN`, then a decision that is the user's.
 *
 * Four calls, and the shape of them is deliberate: three of them **answer with the
 * state the connection is in afterwards**, and the fourth asks for that state on its
 * own. Nothing on this side keeps a flag. The engine is the only thing that knows
 * whether a transaction is open, because it is the only thing that can end one by
 * itself — a statement that fails inside a block leaves PostgreSQL in an *aborted*
 * transaction which refuses everything until it is rolled back, and a client-side
 * boolean would happily go on saying "active" while the connection accepted nothing.
 *
 * Which is why {@link txState} is asked after every statement rather than only after
 * a transaction verb: an open transaction changes what the next statement means, and
 * a failed one changes whether there will be a next statement at all.
 */

import { picus } from '../rpc';

/**
 * Where a connection's transaction stands.
 *
 * `savepoint` is part of the contract but PostgreSQL never reports it: the server's
 * transaction status does not distinguish a block with savepoints from one without,
 * so the provider does not guess. An engine that tracks its own savepoints can.
 */
export type TxState = 'none' | 'active' | 'failed' | 'savepoint';

/** The answer to begin / commit / rollback. */
export interface TxOutcome {
  /** The state the connection is in **after** the call — read back from the server. */
  state: TxState;
  /** What happened, in the user's terms. Shown verbatim. */
  message: string;
  /**
   * Statements that ran inside the transaction that just ended.
   *
   * `0` means **not counted**, never "none ran": the PostgreSQL provider's
   * transaction calls see a connection and no statements, so only a session that
   * counts them can fill this in. Anything rendering it must treat zero as absent.
   */
  statements: number;
}

/** Open an explicit transaction. Rejects, in words, when one is already open. */
export function txBegin(id: string): Promise<TxOutcome> {
  return picus('picus_tx_begin', { id });
}

/**
 * Commit the open transaction.
 *
 * Rejects on a **failed** transaction rather than forwarding it: PostgreSQL's own
 * `COMMIT` inside an aborted block performs a rollback and reports success, which
 * would tell the user their work was saved at the moment it was thrown away.
 */
export function txCommit(id: string): Promise<TxOutcome> {
  return picus('picus_tx_commit', { id });
}

/**
 * Roll the open transaction back — including, and especially, a failed one.
 *
 * The only call here that works in every state, which is what makes it safe to put
 * behind "close anyway". Rolling back with nothing open succeeds and says so.
 */
export function txRollback(id: string): Promise<TxOutcome> {
  return picus('picus_tx_rollback', { id });
}

/** Ask the server where the transaction stands. Cheap: one round trip, no writes. */
export function txState(id: string): Promise<TxState> {
  return picus('picus_tx_state', { id });
}
