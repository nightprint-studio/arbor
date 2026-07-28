/**
 * Picus held results — a window onto a cursor the server is keeping open.
 *
 * A read in Picus does not produce "the rows". It produces a **held cursor** plus
 * the first window onto it, and the grid asks for the rest as you scroll. Two
 * things follow from that, and this file exists to get both right:
 *
 *  • **The length is known before the rows are.** The planner's estimate arrives
 *    with the first window, so the scrollbar is the right size immediately; the
 *    exact count is asked for in the background and replaces it when it lands.
 *    Until then every total is APPROXIMATE and every consumer must mark it `~` —
 *    {@link formatRowTotal} is the one place that formatting lives.
 *  • **A cursor is a resource on someone's database.** It is closed when the tab
 *    closes, when a new statement replaces it, when the connection goes down and
 *    when the window unloads. {@link picusResultsStore} owns every one of those
 *    paths, so no consumer has to remember.
 *
 * ## What is kept in memory
 *
 * Windows are stored as ranges, sorted and non-overlapping, so a jump to row
 * 3 000 000 costs one window rather than everything before it. Past
 * {@link ROW_BUDGET} rows the ranges farthest from where you are reading are
 * dropped — scrolling through a large table must not end as an out-of-memory. A
 * result small enough to fit in the budget is never evicted, which is what lets
 * it report `complete` and hand sorting and filtering back to the grid.
 */

import type { CellValue, Column } from '$lib/types/picus';
import {
  type ExecuteResult,
  closeResult,
  countResult,
  resultWindow,
} from '$lib/ipc/picus/db';
import { picusSettingsStore } from './settings.svelte';

/** Rows kept in memory per result before the farthest ranges are dropped. */
const ROW_BUDGET = 20_000;

/** Window size used when neither the setting nor the first window says anything. */
const FALLBACK_WINDOW = 500;

/** One loaded range. `start` is absolute within the result. */
interface Range {
  start: number;
  rows: CellValue[][];
}

/**
 * A result the grid can be driven from. Satisfies `DataGridWindowSource`
 * structurally — through getters, so every read is reactive at the point the
 * grid reads it rather than at the point the object was handed over.
 */
export interface PicusResult {
  readonly resultId: string;
  readonly connectionId: string;
  readonly columns: Column[];
  /** Rows the result is believed to have. */
  readonly total: number;
  /** `total` is the planner's estimate; the exact count has not landed yet. */
  readonly approximate: boolean;
  /** Every row is in memory and the length is exact. */
  readonly complete: boolean;
  /** Rows actually held. */
  readonly loaded: number;
  /** Server-side time of the statement that opened this result. */
  readonly elapsedMs: number;
  /** The exact count is being computed in the background. */
  readonly counting: boolean;
  /** Last failure while fetching a window; empty when there was none. */
  readonly error: string;
  readonly chunk: number;
  readonly margin: number;
  rowAt(index: number): CellValue[] | undefined;
  request(start: number, count: number): void;
  close(): Promise<void>;
}

/**
 * A total, marked when it is a guess.
 *
 * The `~` is not decoration: an estimate can be out by a factor on a table that
 * has just been written to, and a number that looks counted invites arithmetic
 * nobody should be doing on it.
 */
export function formatRowTotal(result: Pick<PicusResult, 'total' | 'approximate'>): string {
  return `${result.approximate ? '~' : ''}${result.total.toLocaleString()}`;
}

/**
 * Adopt the result a statement opened.
 *
 * Returns `null` for a statement that returned no rows — a write reports what it
 * touched and holds no cursor, and inventing an empty result for it would put a
 * grid where an outcome belongs.
 */
export function createResult(connectionId: string, res: ExecuteResult): PicusResult | null {
  if (!res.resultId) return null;
  const resultId = res.resultId;
  const first = res.rows ?? [];

  // Ranges are held raw: they are replaced wholesale on every arrival and never
  // mutated in place, so a deep proxy over hundreds of thousands of cells would
  // buy nothing and cost on every single read the grid makes.
  let ranges = $state.raw<Range[]>(first.length ? [{ start: 0, rows: first }] : []);
  let loaded = $state(first.length);
  let exactTotal = $state<number | null>(res.totalRows ?? null);
  const estimate = res.estimatedRows ?? 0;
  /** Index one past the last row, once a window has run out. Exact when set. */
  let endIndex = $state<number | null>(res.endOfResult ? first.length : null);
  /**
   * A length we have PROOF of: a window that came back without reaching the end
   * proves there is at least one row after it.
   *
   * The estimate is allowed to be wrong in both directions, and an estimate that
   * is too small is the dangerous one — it would shorten the scrollbar to what is
   * already loaded, and a grid that cannot be scrolled past its last loaded row
   * never asks for another window. This floor is what makes the tail reachable
   * even when the planner is badly out and the exact count never arrives.
   */
  let knownFloor = $state(res.endOfResult ? 0 : first.length + 1);
  let counting = $state(false);
  let error = $state('');

  let closed = false;
  /** Offsets with a window in flight — the single dedup point for the grid's
   *  deliberately repeated asks. */
  const pending = new Set<number>();
  /** Where the reader is, for deciding which ranges to drop first. */
  let focus = 0;

  /**
   * How much to ask for at a time.
   *
   * The user's row limit is no longer a ceiling on what a query returns — it is
   * how much of it is fetched per trip, which is the only thing that number was
   * ever really choosing. Read once, at creation: changing it mid-scroll would
   * re-cut the ranges already held for nothing.
   */
  const windowSize = Math.max(
    1,
    picusSettingsStore.rowLimit || first.length || FALLBACK_WINDOW,
  );

  // Plain functions over `$state` rather than `$derived`: a result is created from
  // an async continuation, outside any component or effect root, and a getter that
  // simply reads the signals it depends on is correct there without relying on how
  // an unowned derived is scheduled. All three are O(ranges) — a few dozen at most.
  function total(): number {
    return endIndex ?? exactTotal ?? Math.max(estimate, loaded, knownFloor);
  }

  function approximate(): boolean {
    return endIndex === null && exactTotal === null;
  }

  /** True when the ranges cover `[0, total)` without a gap. */
  function covered(): boolean {
    let reach = 0;
    for (const r of ranges) {
      if (r.start > reach) return false;
      reach = Math.max(reach, r.start + r.rows.length);
    }
    return reach >= total();
  }

  function complete(): boolean {
    return !approximate() && covered();
  }

  /** The range holding `index`, or `undefined`. Ranges are sorted by `start`. */
  function find(index: number): Range | undefined {
    let lo = 0;
    let hi = ranges.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const r = ranges[mid];
      if (index < r.start) hi = mid - 1;
      else if (index >= r.start + r.rows.length) lo = mid + 1;
      else return r;
    }
    return undefined;
  }

  function rowAt(index: number): CellValue[] | undefined {
    const r = find(index);
    return r ? r.rows[index - r.start] : undefined;
  }

  /** First index in `[from, to)` with no row behind it. */
  function firstGap(from: number, to: number): number | null {
    for (let i = from; i < to; i += 1) if (!find(i)) return i;
    return null;
  }

  /**
   * Insert a window, merging it with whatever it touches.
   *
   * Overlaps are resolved in favour of what is already held: a window that
   * arrives twice must not shift the rows around it, and the server's answer for
   * a row we already have is the same row.
   */
  function insert(start: number, rows: CellValue[][]) {
    if (!rows.length) return;
    const next: Range[] = [];
    let gained = 0;
    let cursor = start;
    let remaining = rows;

    for (const r of ranges) {
      // Emit what of the incoming window sits before this range.
      if (cursor < r.start && remaining.length) {
        const take = Math.min(r.start - cursor, remaining.length);
        next.push({ start: cursor, rows: remaining.slice(0, take) });
        gained += take;
        cursor += take;
        remaining = remaining.slice(take);
      }
      next.push(r);
      // Skip the part the existing range already answers for.
      const rEnd = r.start + r.rows.length;
      if (cursor < rEnd && remaining.length) {
        const skip = Math.min(rEnd - cursor, remaining.length);
        cursor += skip;
        remaining = remaining.slice(skip);
      }
    }
    if (remaining.length) {
      next.push({ start: cursor, rows: remaining });
      gained += remaining.length;
    }

    next.sort((a, b) => a.start - b.start);
    ranges = evict(next, loaded + gained);
    loaded = ranges.reduce((n, r) => n + r.rows.length, 0);
  }

  /**
   * Keep memory bounded by dropping the ranges farthest from where the reader is.
   *
   * A result that fits inside the budget is never touched: dropping a range from
   * one would make it incomplete again and take sorting away from the user after
   * having just given it back.
   */
  function evict(list: Range[], held: number): Range[] {
    if (held <= ROW_BUDGET) return list;
    const byDistance = [...list].sort(
      (a, b) => Math.abs(a.start - focus) - Math.abs(b.start - focus),
    );
    const keep: Range[] = [];
    let kept = 0;
    for (const r of byDistance) {
      if (kept + r.rows.length > ROW_BUDGET && keep.length) continue;
      keep.push(r);
      kept += r.rows.length;
    }
    return keep.sort((a, b) => a.start - b.start);
  }

  /**
   * Ask for a range.
   *
   * Called by the grid on every viewport move that sees a gap, in-flight or not —
   * so the two guards here are the contract: trim the ask to the first row we
   * genuinely lack, and refuse an offset already on the wire.
   */
  function request(start: number, count: number) {
    if (closed || complete()) return;
    const from = firstGap(Math.max(0, start), Math.min(total(), start + count));
    if (from === null || pending.has(from)) return;

    pending.add(from);
    focus = from;
    void resultWindow(connectionId, resultId, from, count)
      .then((w) => {
        // `pending.delete` returning false means this result was reset or closed
        // between the ask and the answer: the rows belong to a cursor nobody is
        // looking at any more, and merging them would corrupt the one that is.
        if (closed || !pending.delete(from)) return;
        // The echoed offset places the rows, not the local one — a window that
        // came back for a different range than asked still lands where it belongs.
        const at = w.offset ?? from;
        const got = w.rows ?? [];
        insert(at, got);
        // An empty window IS the end, whatever the flag says. Without this, a
        // backend that answers a past-the-end offset with `{ rows: [], endOfResult:
        // false }` would leave a gap the grid re-asks for the moment it re-runs —
        // and re-runs on every arrival. One wrong flag, one infinite request loop.
        if (w.endOfResult || !got.length) endIndex = at + got.length;
        else knownFloor = Math.max(knownFloor, at + got.length + 1);
        error = '';
      })
      .catch((e) => {
        pending.delete(from);
        error = String(e);
      });
  }

  /**
   * Replace the estimate with the count, in the background.
   *
   * Never awaited by anything the user is waiting on: the rows are already on
   * screen and the scrollbar is already the right size to within the estimate.
   * A failure is silent on purpose — a cancelled or unsupported count leaves the
   * estimate standing, which is exactly what it is for.
   */
  function beginCount() {
    if (closed || exactTotal !== null || endIndex !== null) return;
    counting = true;
    void countResult(connectionId, resultId)
      .then((r) => { if (!closed) exactTotal = r.total; })
      .catch(() => { /* cancelled, or the engine has no cheap count — estimate stands */ })
      .finally(() => { counting = false; });
  }

  beginCount();

  return {
    resultId,
    connectionId,
    columns: res.columns ?? [],
    get total() { return total(); },
    get approximate() { return approximate(); },
    get complete() { return complete(); },
    get loaded() { return loaded; },
    elapsedMs: res.elapsedMs,
    get counting() { return counting; },
    get error() { return error; },
    get chunk() { return windowSize; },
    // Look a fifth of a window ahead of the viewport before asking. With the
    // default 500-row window that is "start fetching around row 400", which is
    // the behaviour this was asked for — and it scales with the window instead of
    // being a constant that stops making sense at either end.
    get margin() { return Math.max(1, Math.floor(windowSize / 5)); },
    rowAt,
    request,
    async close() {
      if (closed) return;
      closed = true;
      pending.clear();
      ranges = [];
      loaded = 0;
      try {
        await closeResult(connectionId, resultId);
      } catch {
        /* idempotent by contract; a session already gone has nothing to release */
      }
    },
  };
}

/**
 * Every open result, by owner.
 *
 * The owner is the thing whose lifetime the cursor shares — a tab id today. One
 * registry rather than a handle per store is what makes "close it on every path"
 * checkable: the paths are the four methods below, and nothing else holds a
 * result long enough to leak one.
 */
function createResultsStore() {
  let byOwner = $state.raw<Record<string, PicusResult>>({});

  function drop(owner: string) {
    const previous = byOwner[owner];
    if (!previous) return;
    const { [owner]: _gone, ...rest } = byOwner;
    byOwner = rest;
    void previous.close();
  }

  return {
    /** The result on screen for an owner, if it has one. */
    forOwner(owner: string | undefined | null): PicusResult | null {
      if (!owner) return null;
      return byOwner[owner] ?? null;
    },

    /**
     * Hand a result to an owner, closing whatever it held.
     *
     * This is the "a new statement replaces the previous result" path — the most
     * frequent way a cursor would otherwise be abandoned, since running a second
     * query in the same tab looks like nothing was discarded.
     */
    adopt(owner: string, result: PicusResult | null) {
      drop(owner);
      if (result) byOwner = { ...byOwner, [owner]: result };
    },

    /** A tab closed. */
    release(owner: string) {
      drop(owner);
    },

    /** A connection went down, or was deleted: its cursors went with it. */
    releaseConnection(connectionId: string) {
      for (const [owner, result] of Object.entries(byOwner)) {
        if (result.connectionId === connectionId) drop(owner);
      }
    },

    /** The window is going away. */
    releaseAll() {
      for (const owner of Object.keys(byOwner)) drop(owner);
    },
  };
}

export const picusResultsStore = createResultsStore();
