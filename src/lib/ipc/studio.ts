import { invoke } from '@tauri-apps/api/core';

/** File kinds the Studio sidebar can index. Mirrors `StudioFileKind` in
 *  `src-tauri/src/studio/mod.rs` — keep in sync. Phase 5.a adds `yaml`
 *  (covers `.yaml` and `.yml`); Phase 6 adds `properties`. */
export type StudioFileKind = 'ron' | 'json' | 'toml' | 'yaml' | 'properties';

export type SchemaHintOrigin = 'directive' | 'sidecar';

/** Slim mirror of `EntrySchema` in the backend — surfaced as the
 *  schema-binding badge on each `.ron` row in the Studio sidebar. */
export interface EntrySchema {
  rs_file:   string;
  root_type: string;
  origin:    SchemaHintOrigin;
}

export interface StudioFileEntry {
  absolute_path: string;
  /** POSIX-style ("a/b/c.ron") so the frontend can split on `/` safely
   *  regardless of host OS — the backend normalises Windows separators. */
  relative_path: string;
  name:          string;
  kind:          StudioFileKind;
  size_bytes:    number;
  /** `true` when the file matches one of the `excludes` globs from the
   *  repo's `.ron-studio.toml`. */
  excluded?:     boolean;
  /** Schema binding for `.ron` files when one is configured. */
  schema?:       EntrySchema;
  /** `true` when the entry came from a user-registered external
   *  location (file or folder added via "Add external"). The
   *  sidebar groups these under a virtual `external/<label>/`
   *  prefix and decorates them with a link-style icon. */
  external?:     boolean;
}

/** Repo-root `.ron-studio.toml`, normalised. Empty/missing → empty arrays. */
export interface StudioConfig {
  excludes:  string[];
  default?:  SchemaBindingCfg;
  overrides: SchemaOverrideCfg[];
  /** Files/folders outside the repo, registered by the user via the
   *  Studio sidebar's "Add external" action. */
  externals?: ExternalCfg[];
}

export interface ExternalCfg {
  path:   string;
  label?: string;
}

export interface SchemaBindingCfg {
  rs_file:           string;
  root_type:         string;
  /** Custom reference-field patterns; `[]` falls back to convention. */
  reference_fields?: string[];
}

export interface SchemaOverrideCfg {
  glob:              string;
  rs_file:           string;
  root_type:         string;
  reference_fields?: string[];
}

export const studioGetConfig = (tabId: string): Promise<StudioConfig> =>
  invoke<StudioConfig>('studio_get_config', { tabId });

export const studioToggleExclude = (
  tabId:        string,
  relativePath: string,
): Promise<boolean> =>
  invoke<boolean>('studio_toggle_exclude', { tabId, relativePath });

/** Register an external file or folder under the active project.
 *  Path can be absolute or relative; the host canonicalises it
 *  when it exists. `label` is optional — when omitted, the
 *  basename of `path` is used. Idempotent on `path`. */
export const studioAddExternal = (
  tabId: string,
  path:  string,
  label?: string,
): Promise<void> =>
  invoke('studio_add_external', { tabId, path, label });

/** Drop an external registration by path. Returns `true` when the
 *  entry was actually removed (caller may want to skip a no-op
 *  rescan when nothing changed). */
export const studioRemoveExternal = (
  tabId: string,
  path:  string,
): Promise<boolean> =>
  invoke<boolean>('studio_remove_external', { tabId, path });

export const studioBindSchema = (
  tabId:           string,
  relativePath:    string,
  rsFile:          string,
  rootType:        string,
  /** Pass `null` to keep existing patterns; pass `[]` to clear them
   *  back to the built-in convention; pass an array to replace. */
  referenceFields: string[] | null = null,
): Promise<void> =>
  invoke('studio_bind_schema', {
    tabId,
    relativePath,
    rsFile,
    rootType,
    referenceFields,
  });

export const studioUnbindSchema = (
  tabId:        string,
  relativePath: string,
): Promise<boolean> =>
  invoke<boolean>('studio_unbind_schema', { tabId, relativePath });

/** Flip a single field name in the reference-field list of the binding
 *  that matches `relativePath`. Returns the new state: `true` when the
 *  field is now part of the list. Creates a new override when nothing
 *  matches the path yet. */
export const studioToggleReferenceField = (
  tabId:        string,
  relativePath: string,
  field:        string,
): Promise<boolean> =>
  invoke<boolean>('studio_toggle_reference_field', { tabId, relativePath, field });

/** Persistent project-wide cross-ref index toggle + tunables. Mirrors
 *  `crate::config::app_config::StudioSettings`. */
export interface StudioSettings {
  use_index: boolean;
}

export const getStudioSettings = (): Promise<StudioSettings> =>
  invoke<StudioSettings>('get_studio_settings');

export const setStudioSettings = (settings: StudioSettings): Promise<void> =>
  invoke<void>('set_studio_settings', { settings });

/** Fire the background refresh job — IPC returns immediately, progress
 *  + completion arrive via Tauri events (see studio.svelte.ts listeners). */
export const studioRefreshIndex = (tabId: string): Promise<void> =>
  invoke<void>('studio_refresh_index', { tabId });

export interface IndexProgressEvent {
  tab_id:    string;
  processed: number;
  total:     number;
}

export interface IndexDoneEvent {
  tab_id:        string;
  files_indexed: number;
  took_ms:       number;
}

/** Scan the active tab's repository for indexable data files.
 *  Pass an empty `kinds` array to get every supported kind. */
export const studioScanRepo = (
  tabId: string,
  kinds: StudioFileKind[] = [],
): Promise<StudioFileEntry[]> =>
  invoke<StudioFileEntry[]>('studio_scan_repo', { tabId, kinds });

/** A single top-level definition surfaced by the project-wide
 *  cross-ref scanner — see `src-tauri/src/studio/mod.rs::CrossRefDef`.
 *  `kind` was added in Phase 3.c so RON and JSON cross-refs can share
 *  the same IPC + store while keeping their namespaces separate. */
export interface CrossRefDef {
  id_value:      string;
  absolute_path: string;
  relative_path: string;
  file_name:     string;
  kind:          StudioFileKind;
  /** Full AST path of the def — e.g. `["abilities", "2", "id"]` for a
   *  definition nested in a list. The frontend uses this to expand +
   *  select the right node after jumping into the target file. */
  def_path:      string[];
  /** `'id'` or `'name'`. */
  def_field:     string;
}

/** Filter the cross-ref / usage / broken-ref scanners to a specific
 *  set of file kinds. Omit (or pass `null`) to fall back to the
 *  legacy RON-only behaviour. Phase 3.c.a — JSON modal passes
 *  `['json']`. */
export const studioScanCrossRefs = (
  tabId: string,
  kinds: StudioFileKind[] | null = null,
): Promise<CrossRefDef[]> =>
  invoke<CrossRefDef[]>('studio_scan_cross_refs', { tabId, kinds });

/** A reverse hit: a reference field somewhere in the repo whose value
 *  equals the queried target id. Field path matches the AST path scheme
 *  the host uses for `get_children`/`get_value`, so the frontend can
 *  jump straight to it with the existing tree-expand helper. */
export interface UsageMatch {
  absolute_path: string;
  relative_path: string;
  file_name:     string;
  kind:          StudioFileKind;
  field_path:    string[];
  key_name:      string;
}

/** Find every `*_id` / `*_ref` / `target` / … field across the project
 *  whose string value matches `target`. Empty target returns `[]`
 *  without doing any IO. */
export const studioFindUsages = (
  tabId:  string,
  target: string,
  kinds:  StudioFileKind[] | null = null,
): Promise<UsageMatch[]> =>
  invoke<UsageMatch[]>('studio_find_usages', { tabId, target, kinds });

/** A reference field whose value doesn't match any `id`/`name`
 *  definition known to the project — a dangling pointer. Same
 *  field-path scheme as `UsageMatch` (so we can jump to it), plus
 *  the orphan `value` for labelling. Sorted by `value` server-side
 *  so the same broken target groups together visually. */
export interface BrokenRef {
  absolute_path: string;
  relative_path: string;
  file_name:     string;
  kind:          StudioFileKind;
  field_path:    string[];
  key_name:      string;
  value:         string;
}

/** Scan every file in the active repo whose kind matches `kinds`
 *  (defaults to RON-only when omitted) for reference-field values
 *  that point at no known def. Empty result is the happy path —
 *  nothing is broken. */
export const studioScanBrokenRefs = (
  tabId: string,
  kinds: StudioFileKind[] | null = null,
): Promise<BrokenRef[]> =>
  invoke<BrokenRef[]>('studio_scan_broken_refs', { tabId, kinds });
