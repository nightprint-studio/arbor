/**
 * Read-only helpers that turn a `PluginInfo` manifest into the little
 * badge/label strings the Plugin Manager surface renders.
 *
 * They used to be hand-rolled twice — once in `PluginPanel.svelte` (the list)
 * and once in `manager/PluginInfoModal.svelte` (the detail window) — which is
 * how the two copies of `activeHooks` managed to go stale in lockstep. One
 * copy here means the next hook-shape change is one edit.
 */

import type { PluginInfo } from '$lib/types/plugin';

/**
 * The manifest `[hooks]` table as it actually arrives over IPC: a map keyed by
 * fully-qualified hook name, `true` where the plugin opted in.
 *
 * Backend shape: `arbor_plugin_types::hooks::Hooks`, a `#[serde(transparent)]`
 * `BTreeMap<String, bool>`. Keys are namespaced (`arbor:repo_open`,
 * `corvus:commit`, `garrulus:note_saved`, `pipeline:run_request`) and the
 * canonical registry of them is `arbor_plugin_types::hook_names`, validated
 * against `arbor_plugin_types::hook_catalog`. There is deliberately no TS
 * mirror of that list: the set grows per product, and this surface only ever
 * echoes back what the backend sent.
 */
export type HookMap = Record<string, boolean>;

/**
 * The hook names a plugin actually subscribed to.
 *
 * Enumerating the map is the only shape that can display hooks the frontend
 * was never written against — new namespaces, wildcards. A fixed list of
 * known field names renders an empty badge row for everything else, which is
 * indistinguishable from "this plugin subscribes to nothing".
 *
 * The cast is load-bearing until `PluginHooks` in `$lib/types/plugin.ts` (not
 * owned by this module) is replaced by `HookMap`: that interface still models
 * the pre-namespace struct-of-optional-booleans, so every field reads
 * `undefined` against the map that arrives.
 */
export function activeHooks(p: PluginInfo): string[] {
  const hooks = (p.hooks ?? {}) as unknown as HookMap;
  return Object.entries(hooks)
    .filter(([, subscribed]) => subscribed)
    .map(([name]) => name);
}

/** True when an `fs_scope` entry grants unrestricted filesystem reach. */
export function fsScopeIsUnrestricted(scope: string[] | undefined): boolean {
  return Array.isArray(scope) && scope.some(s => s === '*');
}

/** Severity class for the filesystem badge: safe / warn / danger. */
export function fsClass(fs: string, unrestricted: boolean): string {
  if (fs === 'none') return 'safe';
  if (unrestricted)  return 'danger';
  return 'warn';
}

/** Human label for the filesystem badge. */
export function fsLabel(fs: string, scope: string[] | undefined): string {
  if (fs === 'none') return 'no fs';
  return fsScopeIsUnrestricted(scope) ? `fs:${fs} (unrestricted)` : `fs:${fs}`;
}
