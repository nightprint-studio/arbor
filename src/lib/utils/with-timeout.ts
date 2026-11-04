/**
 * Settle `promise`, or reject with a `TimeoutError` after `ms`.
 *
 * For a call whose silence is itself a failure — an IPC to a backend that may not be serving yet
 * never rejects, it just does not answer, and a screen waiting on it would wait forever. The call
 * is not cancelled (an IPC cannot be); its late answer is simply ignored.
 */
export class TimeoutError extends Error {
  constructor(what: string, ms: number) {
    super(`${what} did not answer within ${Math.round(ms / 1000)}s`);
    this.name = 'TimeoutError';
  }
}

export function withTimeout<T>(promise: Promise<T>, ms: number, what = 'The backend'): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new TimeoutError(what, ms)), ms);
    promise.then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); },
    );
  });
}
