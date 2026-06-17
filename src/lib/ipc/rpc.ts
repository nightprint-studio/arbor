import { invoke } from '@tauri-apps/api/core';

/**
 * Generic Model-D IPC bridge.
 *
 * The shell exposes a single `rpc` command; every product backend command is
 * reached as `(program, method, params)` instead of a dedicated Tauri command.
 * Keep the typed wrappers (in the per-domain `ipc/*.ts` files) for ergonomics —
 * they just build the `params` object and call here.
 *
 * `params` keys are the backend handler's argument names, in **snake_case**
 * (they're forwarded verbatim inside the opaque `params` object — Tauri's
 * camelCase→snake_case conversion only applies to a command's direct args, not
 * to a nested JSON payload).
 */
export function rpc<R>(program: string, method: string, params: Record<string, unknown> = {}): Promise<R> {
  return invoke<R>('rpc', { program, method, params });
}

/** Bound helper for the Corvus (git) backend. */
export const corvus = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('corvus', method, params);
