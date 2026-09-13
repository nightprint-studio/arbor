/**
 * Process monitor store — what Arbor's own processes are costing, sampled while the screen is
 * open.
 *
 * ## It polls only while somebody is looking
 *
 * A monitor that sampled in the background would be a process doing work to report that processes
 * are doing work. `start()` is called by the screen when it opens and `stop()` when it closes, and
 * between those two it asks once a second. Nothing here runs otherwise.
 *
 * ## The watchdog needs two samples, not one
 *
 * A language server pins a core for the whole of its first index, and that is the system working
 * rather than the system stuck. So a CPU warning is raised only when a process has held the line
 * across two consecutive samples — which is what `sustained` counts. Memory has no such problem: a
 * process holding three gigabytes is holding three gigabytes.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { processReport, type ProcessReport, type ProcessRow } from '$lib/ipc/processes';
import { platform } from '$lib/ipc/rpc';

/** How often the screen re-asks, in milliseconds. */
const INTERVAL_MS = 1000;

/** What the watchdog considers too much. Mirrors the shell's `ProcessesConfig`. */
export interface ProcessesConfig {
  /** Warn above this many megabytes in one process. `0` is off. */
  warn_mb: number;
  /** Warn above this percent of one core, held across two samples. `0` is off. */
  warn_cpu: number;
}

/** A row plus what the watchdog makes of it. */
export interface WatchedRow extends ProcessRow {
  /** Over the memory line. */
  heavy: boolean;
  /** Over the CPU line, and not for the first sample running. */
  busy: boolean;
}

function createProcessMonitorStore() {
  let report = $state<ProcessReport | null>(null);
  let config = $state<ProcessesConfig>({ warn_mb: 2048, warn_cpu: 90 });
  let error = $state<string | null>(null);
  /** How many consecutive samples each pid has been over the CPU line — see the module doc. */
  let overCpu = $state<Record<number, number>>({});
  let timer: ReturnType<typeof setInterval> | null = null;
  /** How many screens are open on it. Two windows can both be watching. */
  let watchers = 0;

  async function sample() {
    try {
      const next = await processReport();
      error = null;
      const streak: Record<number, number> = {};
      for (const row of next.rows) {
        const over = config.warn_cpu > 0 && next.cpu_sampled && row.cpu >= config.warn_cpu;
        streak[row.pid] = over ? (overCpu[row.pid] ?? 0) + 1 : 0;
      }
      overCpu = streak;
      report = next;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  return {
    get report() { return report; },
    get error() { return error; },
    get config() { return config; },

    /** The rows with the watchdog's verdict on each. */
    get rows(): WatchedRow[] {
      const limit = config.warn_mb * 1024 * 1024;
      return (report?.rows ?? []).map((row) => ({
        ...row,
        heavy: config.warn_mb > 0 && row.memory >= limit,
        busy: (overCpu[row.pid] ?? 0) >= 2,
      }));
    },

    /** Every row the watchdog is unhappy about — what the screen leads with. */
    get warnings(): WatchedRow[] {
      return this.rows.filter((r) => r.heavy || r.busy);
    },

    /** Begin sampling. Reference-counted: two windows watching is one poll each. */
    async start(): Promise<void> {
      watchers += 1;
      if (timer) return;
      try {
        config = await platform<ProcessesConfig>('get_processes_config');
      } catch {
        // The defaults above are the shell's defaults; a config that cannot be read is not a
        // reason to show nothing.
      }
      await sample();
      timer = setInterval(() => void sample(), INTERVAL_MS);
    },

    /** Stop sampling when the last screen closes. */
    stop(): void {
      watchers = Math.max(0, watchers - 1);
      if (watchers > 0 || !timer) return;
      clearInterval(timer);
      timer = null;
    },

    /** Ask again now, without waiting for the next tick. */
    refresh(): Promise<void> {
      return sample();
    },

    /** Persist the thresholds. */
    async setConfig(next: ProcessesConfig): Promise<void> {
      config = next;
      // Re-evaluated on the next tick; the streak is reset here because a threshold that moved
      // makes the count it was built from meaningless.
      overCpu = {};
      await platform('set_processes_config', { config: next }).catch(() => {});
    },
  };
}

export const processMonitorStore = createProcessMonitorStore();
