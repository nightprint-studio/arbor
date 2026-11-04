/**
 * The worker side of the Regex Tester: runs one request at a time, synchronously, off the UI thread.
 * A pattern that backtracks forever only ever stalls *this* thread, which the page terminates.
 *
 * No `webworker` lib reference: the project compiles against the DOM lib, and the two clash. The
 * worker scope is reached through a cast instead.
 */

import { runRegex } from './execute';
import type { RunRequest, RunResult } from './types';

export interface WorkerRequest { id: number; req: RunRequest }
export type WorkerResponse = { id: number; result: RunResult } | { id: number; crash: string };

const scope = self as unknown as Worker;

scope.addEventListener('message', (e: MessageEvent<WorkerRequest>) => {
  const { id, req } = e.data;
  let response: WorkerResponse;
  try {
    response = { id, result: runRegex(req) };
  } catch (err) {
    response = { id, crash: err instanceof Error ? err.message : String(err) };
  }
  scope.postMessage(response);
});
