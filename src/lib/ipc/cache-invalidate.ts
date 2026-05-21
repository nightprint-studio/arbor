/**
 * Thin event hook that IPC mutation wrappers call after a successful write.
 * The cacheStore registers its handler here on init so it can invalidate
 * stale tab data without creating a circular import.
 */

type InvalidateHandler = (tabId: string) => void;
let _handler: InvalidateHandler | null = null;

/** Called once by cacheStore during initialisation. */
export function registerInvalidateHandler(fn: InvalidateHandler): void {
  _handler = fn;
}

/** Called by every write IPC wrapper after a successful operation. */
export function invalidateTabCache(tabId: string): void {
  _handler?.(tabId);
}
