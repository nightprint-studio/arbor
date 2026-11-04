/**
 * Running a pattern without ever blocking the UI: a dedicated worker, a deadline, and cancellation
 * by termination.
 *
 * A regex engine cannot be interrupted from inside — a catastrophic pattern is a tight loop — so the
 * only reliable stop is to kill the thread it runs on. That is what both a timeout and a newer
 * request do: the worker is terminated and a fresh one is started for the next run. A pattern that
 * finishes returns its result; one that does not is reported as timed out, never as a freeze.
 */

import type { WorkerRequest, WorkerResponse } from './regex.worker';
import type { RunRequest, RunResult } from './types';

export type RunOutcome =
  | { kind: 'done'; result: RunResult }
  | { kind: 'timeout'; ms: number }
  | { kind: 'cancelled' }
  | { kind: 'crash'; message: string };

export interface RegexRunner {
  run(req: RunRequest, timeoutMs?: number): Promise<RunOutcome>;
  /** Abandon the run in flight, if any. */
  cancel(): void;
  dispose(): void;
}

const DEFAULT_TIMEOUT_MS = 2000;

export function createRegexRunner(): RegexRunner {
  let worker: Worker | null = null;
  let nextId = 0;
  let pending: { id: number; settle: (o: RunOutcome) => void; timer: ReturnType<typeof setTimeout> } | null = null;

  function spawn(): Worker {
    const w = new Worker(new URL('./regex.worker.ts', import.meta.url), { type: 'module' });
    w.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const msg = e.data;
      if (!pending || msg.id !== pending.id) return;
      const p = pending;
      pending = null;
      clearTimeout(p.timer);
      p.settle('crash' in msg ? { kind: 'crash', message: msg.crash } : { kind: 'done', result: msg.result });
    };
    w.onerror = (e) => {
      if (!pending) return;
      const p = pending;
      pending = null;
      clearTimeout(p.timer);
      p.settle({ kind: 'crash', message: e.message || 'The regex worker failed' });
    };
    return w;
  }

  function kill() {
    worker?.terminate();
    worker = null;
  }

  function abandonPending(outcome: RunOutcome) {
    if (!pending) return;
    const p = pending;
    pending = null;
    clearTimeout(p.timer);
    p.settle(outcome);
    kill();
  }

  return {
    run(req, timeoutMs = DEFAULT_TIMEOUT_MS) {
      abandonPending({ kind: 'cancelled' });
      worker ??= spawn();
      const id = ++nextId;
      return new Promise<RunOutcome>((resolve) => {
        const timer = setTimeout(() => abandonPending({ kind: 'timeout', ms: timeoutMs }), timeoutMs);
        pending = { id, settle: resolve, timer };
        const msg: WorkerRequest = { id, req };
        worker!.postMessage(msg);
      });
    },
    cancel() { abandonPending({ kind: 'cancelled' }); },
    dispose() { abandonPending({ kind: 'cancelled' }); kill(); },
  };
}
