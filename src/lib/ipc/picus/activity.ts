/**
 * Picus session monitor IPC — what the server is doing, and stopping one of it.
 *
 * A file of its own rather than three more exports in `db.ts`: this is the only
 * part of the surface that describes the *server* rather than a schema or a
 * statement, it is the only one that is polled, and it is the only one that can
 * end somebody else's transaction.
 *
 * The shapes mirror `picus_db_api::activity` field for field — the Rust side
 * serialises camelCase precisely so there is no translation layer here.
 */

import { picus } from '../rpc';

/** One backend connected to the server. */
export interface SessionActivity {
  /** Server-side process id — the handle every action takes. */
  pid: number;
  user: string;
  database: string;
  /** What the client called itself: `psql`, `Picus`, an application name. */
  application: string;
  /** Client address; empty for a connection over the local socket. */
  client: string;
  /** `active`, `idle`, `idle in transaction`, … as the server words it. */
  state: string;
  /** What it is waiting on. `null` means it is running, not that it is idle. */
  waitEvent: string | null;
  /** The statement, as the server holds it — it may have truncated it itself. */
  query: string;
  /**
   * Ages in milliseconds, computed **by the server**. Never derive these from the
   * browser's clock: the two machines disagree, sometimes by hours, and a wrong
   * duration here looks like a finding rather than like a bug.
   */
  queryAgeMs: number | null;
  /** How long it has been in its current state — what identifies an abandoned
   *  `idle in transaction`. */
  stateAgeMs: number | null;
  transactionAgeMs: number | null;
  /** Picus's own session. Legal to stop, but it has to be labelled. */
  isSelf: boolean;
  /** Pids this session is waiting for; empty when it is not blocked. */
  blockedBy: number[];
}

/** One "waiter is stuck behind blocker" relationship. */
export interface BlockEdge {
  waiter: number;
  blocker: number;
  /** The object being contended, when the server names one. */
  relation: string | null;
  /** The lock mode being waited for. */
  mode: string | null;
}

/** Everything the monitor shows, from one read. */
export interface ActivitySnapshot {
  sessions: SessionActivity[];
  /** Who is waiting for whom. Empty when nothing is blocked. */
  blocked: BlockEdge[];
  /** The server's own idea of when this was read, as it formats it. Displayed,
   *  never computed with. */
  readAt: string;
}

/**
 * How firmly to ask a session to stop.
 *
 * Two verbs, not one: `cancel` asks the *statement* to stop and leaves the
 * connection alive, which is almost always what is wanted; `terminate` drops the
 * connection and **rolls its transaction back**, and is the answer only for a
 * session that is not running anything — the abandoned `idle in transaction`
 * holding a lock.
 */
export type StopKind = 'cancel' | 'terminate';

/** One coherent picture of the server: sessions plus the blocking graph. */
export function readActivity(connectionId: string): Promise<ActivitySnapshot> {
  return picus('picus_activity', { id: connectionId });
}

/**
 * Ask one backend to stop.
 *
 * Resolves `false` when the server found no such pid — the session ended between
 * the read and the click, which is ordinary and not an error. A refusal for want
 * of privilege **rejects**, carrying the server's own sentence: "you may not" and
 * "it was already gone" must not look the same to whoever pressed the button.
 */
export function stopSession(
  connectionId: string,
  pid: number,
  kind: StopKind,
): Promise<boolean> {
  return picus('picus_stop_session', { id: connectionId, pid, kind });
}
