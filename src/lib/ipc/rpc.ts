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
/** The backend product labels the router dispatches to. */
export type Program = 'corvus' | 'platform' | 'studio' | 'merula' | 'sitta' | 'tyto';

export function rpc<R>(program: Program, method: string, params: Record<string, unknown> = {}): Promise<R> {
  return invoke<R>('rpc', { program, method, params });
}

/** Bound helper for the Corvus (git) backend. */
export const corvus = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('corvus', method, params);

/** Bound helper for the Platform backend (config, theme, workspace,
 *  jobs, fs, terminal, app metadata — everything that isn't a product). */
export const platform = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('platform', method, params);

/** Bound helper for the Studio backend (standalone CI/pipeline-config editor:
 *  YAML convert/format/validate, schema reflection). */
export const studio = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('studio', method, params);

/** Bound helper for the Merula (music live-coding) backend — served
 *  out-of-process by `merula-be`: eval/transport/render, sample packs, project
 *  model, config, audio devices. Push events (`merula:*`) still arrive over
 *  `listen`, scoped to the merula window — they don't go through this helper. */
export const merula = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('merula', method, params);

/** Bound helper for the Sitta (file explorer) backend — served out-of-process by
 *  `sitta-be`, spawned lazily when an explorer window opens. Today it serves the
 *  explorer's git awareness (`fs_git_*`); the rest of the explorer's FS still goes
 *  through `platform`. Routes to a down overlay when `sitta-be` isn't running. */
export const sitta = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('sitta', method, params);

/** Bound helper for the Tyto (screen recorder) backend — served out-of-process by
 *  `tyto-be`, spawned lazily when the Tyto window opens. Serves the recorder
 *  domains (config, sources, session, region, library). The capture handlers are
 *  stubs until the recording engine lands, so calls resolve empty / reject until
 *  then; `BackendNotRunning` when `tyto-be` isn't up. */
export const tyto = <R>(method: string, params: Record<string, unknown> = {}): Promise<R> =>
  rpc<R>('tyto', method, params);
