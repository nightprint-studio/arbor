import { invoke } from '@tauri-apps/api/core';
import type { BrpConnectParams, BrpStatus } from '$lib/types/brp';

/**
 * Probe the endpoint with `rpc.discover` and, on success, install it as the
 * singleton active session. Rejects with a {@link BrpCallError}-shaped object
 * — caller is responsible for `try/catch` and surfacing the error.kind to the
 * user when relevant.
 */
export const brpConnect = (params: BrpConnectParams = {}) =>
  invoke<BrpStatus>('brp_connect', { params });

export const brpDisconnect = () =>
  invoke<BrpStatus>('brp_disconnect');

export const brpStatus = () =>
  invoke<BrpStatus>('brp_status');

/**
 * Raw JSON-RPC pass-through. `method` is one of `BrpMethod.*`, `params` is the
 * BRP-spec payload (shape varies per method). Returns the unwrapped `result`
 * payload as opaque JSON — typing belongs to the caller since BRP responses
 * are highly polymorphic.
 */
export const brpCall = <T = unknown>(method: string, params?: unknown) =>
  invoke<T>('brp_call', { params: { method, params } });
