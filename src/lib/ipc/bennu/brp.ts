/**
 * The live Bevy inspector — Bennu talking to a running game over the Bevy Remote Protocol.
 *
 * Thin `bennu(...)` wrappers over the `brp` module of `bennu-be`, plus the two push topics. The
 * backend resolves which method names the connected game speaks (`world.*` on Bevy 0.17+,
 * `bevy/*` before), so nothing here — and nothing in the UI — knows a Bevy version.
 *
 * Errors are the game's own message as a rejected string; callers show it as is.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';

// ── Connection ───────────────────────────────────────────────────────────────

export type BrpLink = 'connected' | 'reconnecting' | 'disconnected';

/** What the connected game answers to — the UI offers exactly these verbs. */
export interface BrpCan {
  watch: boolean;
  spawn: boolean;
  despawn: boolean;
  insert: boolean;
  remove: boolean;
  mutate: boolean;
  reparent: boolean;
  resources: boolean;
  mutate_resource: boolean;
  insert_resource: boolean;
  remove_resource: boolean;
}

export interface BrpStatus {
  connected: boolean;
  state: BrpLink;
  endpoint: string | null;
  /** `world` (Bevy 0.17+), `bevy` (0.15 / 0.16) or `unknown`. Informational. */
  family: 'world' | 'bevy' | 'unknown';
  discovered: boolean;
  method_count: number;
  type_count: number;
  component_types: number;
  resource_types: number;
  can: BrpCan;
  /** Changes on every (re)connect; events carry it so stale frames can be dropped. */
  generation: number;
  /** Not a loopback address: writes on a remote machine deserve a visible warning. */
  non_loopback: boolean;
}

/** Payload of {@link BRP_STATE}. `toast` is decided by the backend (one `disconnect_notified` gate). */
export interface BrpStateEvent {
  status: BrpStatus;
  toast: 'lost' | 'regained' | 'gave_up' | null;
  attempt: number;
  max_attempts: number;
  message: string | null;
}

export const BRP_STATE = 'arbor://bennu/brp-state';
export const BRP_ENTITY = 'arbor://bennu/brp-entity';

export const brpConnect = (endpoint?: string): Promise<BrpStatus> =>
  bennu('bennu_brp_connect', { args: { endpoint: endpoint?.trim() || undefined } });
export const brpDisconnect = (): Promise<BrpStatus> => bennu('bennu_brp_disconnect', {});
export const brpStatus = (): Promise<BrpStatus> => bennu('bennu_brp_status', {});

// ── Entities ─────────────────────────────────────────────────────────────────

export interface BrpRefresh {
  /** Opaque; compare for equality. */
  revision: string;
  total: number;
  /** An entity appeared, vanished, was renamed or reparented since the last refresh. */
  changed: boolean;
  generation: number;
}

export interface BrpRow {
  /** What a marker component says the entity is (`camera`, `light`, …), when one does. */
  kind?: string | null;
  id: number;
  name: string | null;
  parent: number | null;
  child_count: number;
}

export interface BrpEntities {
  revision: string;
  total: number;
  rows: BrpRow[];
  /** Ids in `rows` that are there only as ancestors of a hit. */
  context: number[];
  generation: number;
}

export interface BrpEntitiesArgs {
  /** Children of this entity; omitted = the roots. Ignored while searching or filtering. */
  parent?: number;
  search?: string;
  /** Only entities having every one of these component type paths. */
  with?: string[];
  /** Only entities having none of these. */
  without?: string[];
  offset?: number;
  limit?: number;
  /** With a search or filter: also return each hit's ancestors (see `context`), to draw the hits as a tree. */
  with_parents?: boolean;
}

export interface BrpEntity {
  id: number;
  name: string | null;
  parent: number | null;
  /** type path → value, as the game serialises it. */
  components: Record<string, unknown>;
  /** type path → error object, for components the game could not serialise. */
  errors: Record<string, unknown>;
  generation: number;
}

/** Re-read the world's entity names and hierarchy — the one call that costs the game O(entities). */
export const brpRefresh = (): Promise<BrpRefresh> => bennu('bennu_brp_refresh', {});
export const brpEntities = (args: BrpEntitiesArgs): Promise<BrpEntities> =>
  bennu('bennu_brp_entities', { args });
/** Ancestors of an entity, outermost first — what to expand to reveal a search hit. */
export const brpAncestors = (id: number): Promise<number[]> => bennu('bennu_brp_ancestors', { args: { id } });
export const brpEntity = (id: number): Promise<BrpEntity> => bennu('bennu_brp_entity', { args: { id } });

// ── Watching ─────────────────────────────────────────────────────────────────

/** Payload of {@link BRP_ENTITY}: a frame of the watched entity. */
export type BrpEntityEvent =
  | { kind: 'values'; entity: number; generation: number; components: Record<string, unknown>; errors: Record<string, unknown>; removed: string[] }
  /** A component was added or removed: watch again (which re-lists). */
  | { kind: 'shape'; entity: number; generation: number }
  | { kind: 'error'; entity: number; generation: number; message: string }
  | { kind: 'closed'; entity: number; generation: number };

export const brpWatchEntity = (id: number): Promise<void> => bennu('bennu_brp_watch_entity', { args: { id } });
export const brpUnwatch = (): Promise<void> => bennu('bennu_brp_unwatch', {});

// ── Resources ────────────────────────────────────────────────────────────────

export interface BrpResourceEntry {
  type_path: string;
  short_path: string;
}

export const brpResources = (): Promise<BrpResourceEntry[]> => bennu('bennu_brp_resources', {});
export const brpResource = (typePath: string): Promise<{ type_path: string; value: unknown }> =>
  bennu('bennu_brp_resource', { args: { type_path: typePath } });

// ── Type registry ────────────────────────────────────────────────────────────

export interface BrpTypeInfo {
  type_path: string;
  short_path: string;
  crate_name: string | null;
  reflect_types: string[];
  kind: string | null;
}

export type BrpTypeKind = 'component' | 'resource';

export interface BrpTypes {
  total: number;
  items: BrpTypeInfo[];
}

/** What a typed editor needs to know about a type — a hint, never a contract: BRP serialises through a
 *  type's own `Serialize` when it has one, so a `Vec3` is a struct here but `[x, y, z]` on the wire. */
export type BrpShape =
  | { kind: 'bool' | 'int' | 'float' | 'string' | 'entity' | 'json' }
  | { kind: 'struct'; type_path: string; fields: BrpField[] }
  | { kind: 'tuple'; type_path: string; items: BrpShape[] }
  | { kind: 'enum'; type_path: string; variants: BrpVariant[] }
  | { kind: 'list'; item: BrpShape }
  | { kind: 'map'; key: BrpShape; value: BrpShape };

export interface BrpField {
  name: string;
  shape: BrpShape;
}

/** One enum variant. `sample` is the wire value that selects it — written as is when picked. */
export type BrpVariant = { name: string; sample: unknown } & (
  | { kind: 'unit' }
  | { kind: 'tuple'; items: BrpShape[] }
  | { kind: 'struct'; fields: BrpField[] }
);

export interface BrpShapeInfo {
  shape: BrpShape;
  /** A well-formed placeholder value of the type — where "insert" starts. */
  skeleton: unknown;
  info: BrpTypeInfo | null;
}

export const brpTypes = (args: { kind?: BrpTypeKind; query?: string; offset?: number; limit?: number }): Promise<BrpTypes> =>
  bennu('bennu_brp_types', { args });
export const brpTypeShape = (typePath: string): Promise<BrpShapeInfo> =>
  bennu('bennu_brp_type_shape', { args: { type_path: typePath } });

// ── Writes ───────────────────────────────────────────────────────────────────
// `entity` present = a component of that entity; absent = a resource.

/** Set one field. `path` is a reflect path inside the value: `.translation.x`, `.0`, `.items[2]`. */
export const brpMutate = (a: { entity?: number; type_path: string; path: string; value: unknown }): Promise<void> =>
  bennu('bennu_brp_mutate', { args: a });
/** Insert (or replace whole) a component, or insert a resource. */
export const brpInsert = (a: { entity?: number; type_path: string; value: unknown }): Promise<void> =>
  bennu('bennu_brp_insert', { args: a });
export const brpRemove = (a: { entity?: number; type_path: string }): Promise<void> =>
  bennu('bennu_brp_remove', { args: a });
export const brpSpawn = (components: Record<string, unknown> = {}): Promise<number> =>
  bennu('bennu_brp_spawn', { args: { components } });
export const brpDespawn = (entity: number): Promise<void> => bennu('bennu_brp_despawn', { args: { entity } });
/** Move under `parent`, or detach when omitted. */
export const brpReparent = (entity: number, parent?: number): Promise<void> =>
  bennu('bennu_brp_reparent', { args: { entity, parent } });

// ── Console ──────────────────────────────────────────────────────────────────

/** The methods the connected game lists in `rpc.discover`, sorted. */
export const brpMethods = (): Promise<string[]> => bennu('bennu_brp_methods', {});
/** Call any method with raw params; resolves with the game's `result`, rejects with its error message. */
export const brpCall = (method: string, params?: unknown): Promise<unknown> =>
  bennu('bennu_brp_call', { args: { method, params } });

// ── Driving the game (the `arbor-bevy-remote` companion plugin) ──────────────
// A game without the plugin rejects these with a sentence that says what to add.

export interface BrpScreenshot {
  mime_type: 'image/png';
  /** Base64, no data-URI prefix. */
  data: string;
  width: number;
  height: number;
}

export interface BrpFrameStats {
  frames: number;
  avg_ms: number;
  min_ms: number;
  max_ms: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  fps: number;
}

export interface BrpStats {
  uptime_s: number | null;
  frame: BrpFrameStats | null;
  frame_ms_recent: number[];
  diagnostics: { path: string; value: number | null; average: number | null; smoothed: number | null; suffix: string }[];
  window: { title: string; width: number; height: number; physical_width: number; physical_height: number; scale_factor: number; focused: boolean } | null;
}

export interface BrpDriveInfo {
  version: string;
  bevy: string;
  methods: string[];
  keys: string[];
  gamepad_buttons: string[];
  gamepad_axes: string[];
  actions: { name: string; description: string }[];
  presence: { enabled: boolean; state: string; label: string } | null;
}

export type BrpMouseArgs = {
  action: 'move' | 'down' | 'up' | 'click' | 'double_click' | 'drag' | 'wheel';
  x?: number; y?: number; to_x?: number; to_y?: number;
  button?: 'left' | 'right' | 'middle' | 'back' | 'forward';
  steps?: number; duration_ms?: number; dx?: number; dy?: number; unit?: 'line' | 'pixel';
};
export type BrpKeyArgs = { key: string; action?: 'press' | 'down' | 'up'; modifiers?: string[]; hold_ms?: number };
export type BrpTextArgs = { text: string; interval_ms?: number };
export type BrpPadArgs = { button?: string; action?: 'press' | 'down' | 'up'; axis?: string; value?: number; hold_ms?: number };
export type BrpPresenceArgs = { state?: 'idle' | 'connected' | 'paused'; label?: string; enabled?: boolean };

export const brpDriveInfo = (): Promise<BrpDriveInfo> => bennu('bennu_brp_arbor_info', {});
export const brpScreenshot = (maxWidth?: number): Promise<BrpScreenshot> =>
  bennu('bennu_brp_screenshot', { args: maxWidth ? { max_width: maxWidth } : {} });
export const brpStats = (history = 0): Promise<BrpStats> => bennu('bennu_brp_stats', { args: history ? { history } : {} });
export const brpKey = (a: BrpKeyArgs): Promise<unknown> => bennu('bennu_brp_key', { args: a });
export const brpText = (a: BrpTextArgs): Promise<unknown> => bennu('bennu_brp_text', { args: a });
export const brpMouse = (a: BrpMouseArgs): Promise<unknown> => bennu('bennu_brp_mouse', { args: a });
export const brpGamepad = (a: BrpPadArgs): Promise<unknown> => bennu('bennu_brp_gamepad', { args: a });
export const brpInputCancel = (): Promise<unknown> => bennu('bennu_brp_input_cancel', {});
export const brpAction = (name: string, arg?: unknown): Promise<unknown> =>
  bennu('bennu_brp_action', { args: arg === undefined ? { name } : { name, arg } });
export const brpActions = (): Promise<{ actions: { name: string; description: string }[] }> => bennu('bennu_brp_actions', {});
export const brpPresence = (a: BrpPresenceArgs = {}): Promise<{ enabled: boolean; state: string; label: string }> =>
  bennu('bennu_brp_presence', { args: a });

/** `type path → { field path → hint }` from the game's `#[derive(RemoteInspect)]` types; `{}` without the plugin. */
export const brpInspectHints = (): Promise<import('$lib/components/bennu/brp/brp-widgets').HintMap> =>
  bennu('bennu_brp_inspect_hints', {});

// ── Saved endpoints and searches ─────────────────────────────────────────────

export interface BrpSavedEndpoint {
  name: string;
  endpoint: string;
}

/** A search-box string (`goblin has:Health`) with a name. */
export interface BrpSavedQuery {
  name: string;
  search: string;
}

export interface BrpSaved {
  endpoints: BrpSavedEndpoint[];
  queries: BrpSavedQuery[];
}

export const brpSavedGet = (): Promise<BrpSaved> => bennu('bennu_brp_saved_get', {});
/** Replace what is saved; resolves with what was stored (trimmed, de-duplicated, capped). */
export const brpSavedSet = (saved: BrpSaved): Promise<BrpSaved> => bennu('bennu_brp_saved_set', { args: saved });

// ── Events ───────────────────────────────────────────────────────────────────

export const onBrpState = (cb: (e: BrpStateEvent) => void): Promise<UnlistenFn> =>
  listen<BrpStateEvent>(BRP_STATE, (e) => cb(e.payload));
export const onBrpEntity = (cb: (e: BrpEntityEvent) => void): Promise<UnlistenFn> =>
  listen<BrpEntityEvent>(BRP_ENTITY, (e) => cb(e.payload));
