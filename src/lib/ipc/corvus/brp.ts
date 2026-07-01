import type {
  BrpCallOutcome,
  BrpConnectOutcome,
  BrpConnectParams,
  BrpStatus,
} from '$lib/types/corvus/brp';
import { corvus } from '../rpc';

/**
 * Probe the endpoint with `rpc.discover` and, on success, install it as the
 * singleton active session. Resolves with a {@link BrpConnectOutcome}: the new
 * status on success, or the structured BRP error in the `err` arm (the seam
 * can't carry a structured rejection, so the error rides the success channel).
 */
export const brpConnect = (params: BrpConnectParams = {}) =>
  corvus<BrpConnectOutcome>('brp_connect', { params });

export const brpDisconnect = () =>
  corvus<BrpStatus>('brp_disconnect');

export const brpStatus = () =>
  corvus<BrpStatus>('brp_status');

/**
 * Raw JSON-RPC pass-through. `method` is one of `BrpMethod.*`, `params` is the
 * BRP-spec payload (shape varies per method). Resolves with a
 * {@link BrpCallOutcome} — the unwrapped `result` payload (opaque JSON, typing
 * belongs to the caller) in the `ok` arm, or the structured error in `err`.
 */
export const brpCall = (method: string, params?: unknown) =>
  corvus<BrpCallOutcome>('brp_call', { params: { method, params } });
