/**
 * Picus session monitor — what every backend on the server is doing.
 *
 * The store that polls, and the only one in Picus that does. Two rules shape it,
 * and both are about somebody else's production database:
 *
 * * **It polls only while the panel is on screen.** `start` is called when the
 *   panel mounts and `stop` when it unmounts, so a dock closed on the Consistency
 *   tab costs nothing. A timer that keeps running behind a hidden panel is a query
 *   against a live server every three seconds, forever, for a picture nobody is
 *   looking at.
 * * **One request at a time.** A server that answers slower than the interval
 *   would otherwise accumulate overlapping reads down the same connection, each
 *   waiting on the ones before it — the failure `schemaStore` documents, arriving
 *   here on a timer instead of on a render.
 *
 * The blocking *graph* is turned into a per-row chain here rather than in the
 * panel: "blocked" is not actionable, "blocked by 4412, which is behind 4380,
 * which is idle in transaction" is — and the session at the end of that walk is
 * the only one worth acting on.
 */

import {
  readActivity,
  stopSession,
  type ActivitySnapshot,
  type SessionActivity,
  type StopKind,
} from '$lib/ipc/picus/activity';
import { connectionsStore } from './connections.svelte';
import { picusProvidersStore } from './providers.svelte';

/**
 * How often the snapshot is re-read.
 *
 * Three seconds is the compromise: long enough that the read is negligible beside
 * anything else the server is doing, short enough that a monitor is worth having —
 * a lock you have to wait ten seconds to see is one you go and look at in psql.
 */
const REFRESH_MS = 3000;

const EMPTY: ActivitySnapshot = { sessions: [], blocked: [], readAt: '' };

/** One session, placed in the wait graph. */
export interface ActivityRow {
  session: SessionActivity;
  /**
   * The pids between this session and the one at the root of the wait, nearest
   * first. Empty when this session is not blocked.
   *
   * Follows the **first** blocker at each step. A session can be behind several
   * holders of the same lock — they are all in `session.blockedBy` — but a row can
   * only draw one path, and the first is the one the server named first.
   */
  chain: number[];
  /** Something is waiting behind this session, and nothing is in front of it —
   *  the only session in the chain that acting on will actually release anything. */
  isRoot: boolean;
  /** The walk came back on itself: a deadlock, or a graph read mid-change. */
  cyclic: boolean;
}

/**
 * Walk from a blocked session up to the one at the root of the wait.
 *
 * The visited set is not defensive tidiness: PostgreSQL detects and breaks real
 * deadlocks, but `pg_blocking_pids` is read per row of one scan and a cycle can
 * appear in a snapshot taken while the graph is changing. Without the guard that
 * is an infinite loop inside a `$derived`, which is to say a frozen window.
 */
function walk(pid: number, byPid: Map<number, SessionActivity>): { chain: number[]; cyclic: boolean } {
  const chain: number[] = [];
  const seen = new Set<number>([pid]);
  let current = byPid.get(pid);
  while (current && current.blockedBy.length) {
    const next = current.blockedBy[0];
    if (seen.has(next)) return { chain, cyclic: true };
    seen.add(next);
    chain.push(next);
    current = byPid.get(next);
  }
  return { chain, cyclic: false };
}

function createActivityStore() {
  let snapshot = $state<ActivitySnapshot>(EMPTY);
  /** Which connection the snapshot describes — '' when nothing has been read. */
  let connectionId = $state('');
  let error = $state('');
  /** True only for the FIRST read: a refresh must not blank a table being read. */
  let loading = $state(false);

  /**
   * Deliberately not `$state`: they gate the polling rather than being rendered,
   * and making them reactive would let a read of the table re-enter the timer.
   */
  let timer: ReturnType<typeof setInterval> | null = null;
  let inFlight = false;
  /** The connection the request in flight is for — a late answer for a connection
   *  the user has left describes the wrong server and is dropped. */
  let asking = '';

  const byPid = $derived(new Map(snapshot.sessions.map((s) => [s.pid, s])));
  /** Pids something is waiting behind — what makes a session a root rather than
   *  merely unblocked. */
  const blockers = $derived(new Set(snapshot.sessions.flatMap((s) => s.blockedBy)));

  const rows = $derived<ActivityRow[]>(
    snapshot.sessions.map((session) => {
      const { chain, cyclic } = walk(session.pid, byPid);
      return {
        session,
        chain,
        isRoot: blockers.has(session.pid) && session.blockedBy.length === 0,
        cyclic,
      };
    }),
  );

  async function refresh(id: string) {
    if (!id || inFlight) return;
    inFlight = true;
    asking = id;
    try {
      const read = await readActivity(id);
      // The active connection can change while a read is out. Landing it anyway
      // would file one server's sessions under another's name — and this is a panel
      // whose buttons end transactions.
      if (asking !== id) return;
      snapshot = read;
      connectionId = id;
      error = '';
    } catch (e) {
      if (asking !== id) return;
      error = String(e);
    } finally {
      // Cleared unconditionally, late answer or not: a flag left set by an answer
      // nobody wanted is a monitor that silently stops refreshing.
      inFlight = false;
      loading = false;
    }
  }

  return {
    get rows() { return rows; },
    get sessions() { return snapshot.sessions; },
    get blocked() { return snapshot.blocked; },
    get readAt() { return snapshot.readAt; },
    get connectionId() { return connectionId; },
    get loading() { return loading; },
    get error() { return error; },
    /** Sessions waiting on someone — the number the panel leads with. */
    get blockedCount() { return snapshot.sessions.filter((s) => s.blockedBy.length > 0).length; },

    /**
     * Whether the active connection's engine can answer this at all.
     *
     * Read from the engine's declared capability, never from its name: a monitor
     * that is absent on an engine without the concept is honest, one that is
     * present and fails on every refresh is not.
     *
     * Through `picusProvidersStore` — the one place the descriptors are read.
     * This store used to fetch and cache them itself, and so did the transaction
     * store and the dependency store, which is four copies of one document and
     * four retry policies to keep in step. The descriptors describe what the
     * *build* supports, so there is exactly one right answer and one place to
     * hold it.
     */
    get supported(): boolean {
      // A pure read. Priming the descriptors from here is what turned a backend
      // that was slow to answer into an RPC storm: the getter is read from a
      // `$derived` on the rail, so every failure re-rendered and re-asked.
      // `PicusShell` primes them once, in an effect.
      const engine = connectionsStore.active?.dialect;
      return picusProvidersStore.capabilities(engine)?.sessionActivity ?? false;
    },

    /** Begin polling one connection. Reading it immediately: a panel that opens on
     *  an empty table for three seconds reads as broken. */
    start(id: string) {
      this.stop();
      if (!id) return;
      if (id !== connectionId) {
        // A different server: keep nothing. The pids of the previous one name
        // different processes here, and a stale row is a row with a Terminate
        // button on it.
        snapshot = EMPTY;
        error = '';
        loading = true;
      }
      void refresh(id);
      timer = setInterval(() => void refresh(id), REFRESH_MS);
    },

    /** Stop polling. Called when the panel unmounts — see the note at the top. */
    stop() {
      if (timer !== null) clearInterval(timer);
      timer = null;
    },

    /** Read once, now — the refresh button, and whatever a stop leaves behind. */
    async refreshNow() {
      await refresh(connectionId || connectionsStore.activeId);
    },

    /**
     * Ask one backend to stop, and re-read straight away.
     *
     * Returns the server's own words on refusal and `''` on success, rather than
     * throwing: the caller is a confirmation dialog that has to say what happened
     * either way. `false` from the server means the pid was already gone, which is
     * ordinary — the list is three seconds old by construction.
     */
    async stopBackend(pid: number, kind: StopKind): Promise<{ ok: boolean; found: boolean; reason: string }> {
      const id = connectionId || connectionsStore.activeId;
      if (!id) return { ok: false, found: false, reason: 'No connection is open.' };
      try {
        const found = await stopSession(id, pid, kind);
        await refresh(id);
        return { ok: true, found, reason: '' };
      } catch (e) {
        return { ok: false, found: false, reason: String(e) };
      }
    },

    /** Forget everything — on disconnect, so no dead server's pids stay clickable. */
    clear() {
      this.stop();
      snapshot = EMPTY;
      connectionId = '';
      error = '';
      loading = false;
      asking = '';
    },
  };
}

export const activityStore = createActivityStore();

/**
 * An age as a person reads it: `1.4s`, `2m 10s`, `3h 12m`.
 *
 * Exported because the panel is not the only thing that will want it, and because
 * the rule it encodes — seconds up to a minute, never a bare millisecond count —
 * is the difference between "how long has this been running" being answerable at a
 * glance and being arithmetic.
 */
export function formatAge(ms: number | null): string {
  if (ms === null || ms < 0) return '—';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${Math.floor(seconds % 60)}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}
