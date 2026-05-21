import type { BrpCallError, BrpConnectParams, BrpStatus } from '$lib/types/brp';
import { brpCall, brpConnect, brpDisconnect, brpStatus } from '$lib/ipc/brp';

/**
 * Singleton BRP store — Phase 1.0.
 *
 * Holds the current session status (connected/endpoint/connected_at) plus a
 * last-error slot for surfacing connect/call failures in the UI. The plan
 * (decision #2) calls for a global session, not per-tab — so this is a
 * module-level singleton on purpose. Per-tab state arrives only if usage
 * demands it.
 *
 * Phase 1.1 adds: entity cache + schema cache + selection + polling refresh.
 * Phase 2 adds: SSE watch subscriptions + auto-reconnect state machine.
 */
function createBrpStore() {
  let status    = $state<BrpStatus>({ connected: false });
  let lastError = $state<BrpCallError | null>(null);
  /** True while a connect / call is in flight. */
  let busy      = $state<boolean>(false);

  async function refreshStatus(): Promise<BrpStatus> {
    try {
      status = await brpStatus();
    } catch (e) {
      // brp_status returns an AppError on mutex poisoning — log but don't
      // clobber the cached status. Status reads happen often enough that
      // we'd rather show stale-but-valid data than flicker.
      console.warn('[brp] refreshStatus failed', e);
    }
    return status;
  }

  async function connect(params: BrpConnectParams = {}): Promise<boolean> {
    busy = true;
    lastError = null;
    try {
      status = await brpConnect(params);
      return status.connected;
    } catch (e: unknown) {
      lastError = normaliseError(e);
      status = { connected: false };
      return false;
    } finally {
      busy = false;
    }
  }

  async function disconnect(): Promise<void> {
    busy = true;
    try {
      status = await brpDisconnect();
    } catch (e) {
      console.warn('[brp] disconnect failed', e);
    } finally {
      busy = false;
    }
  }

  async function call<T = unknown>(method: string, params?: unknown): Promise<T | null> {
    if (!status.connected) {
      lastError = {
        kind: 'not_connected',
        message: 'BRP not connected',
      };
      return null;
    }
    try {
      return await brpCall<T>(method, params);
    } catch (e: unknown) {
      lastError = normaliseError(e);
      return null;
    }
  }

  function clearError(): void {
    lastError = null;
  }

  return {
    get status()    { return status; },
    get lastError() { return lastError; },
    get busy()      { return busy; },
    refreshStatus,
    connect,
    disconnect,
    call,
    clearError,
  };
}

export const brpStore = createBrpStore();

function normaliseError(e: unknown): BrpCallError {
  // Tauri's invoke() rejects with the error variant of the command's Result
  // — for brp_connect/brp_call that's the BrpCallError struct, which arrives
  // as a plain object. AppError variants (mutex poisoning, etc.) arrive as
  // plain strings. Try to recognise both shapes.
  if (typeof e === 'object' && e !== null) {
    const obj = e as Record<string, unknown>;
    if (typeof obj.kind === 'string' && typeof obj.message === 'string') {
      return obj as unknown as BrpCallError;
    }
  }
  if (typeof e === 'string') {
    return { kind: 'internal', message: e };
  }
  return { kind: 'internal', message: String(e ?? 'unknown error') };
}
