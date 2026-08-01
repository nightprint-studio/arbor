/**
 * Garrulus (notes) IPC — the typed surface of `garrulus-be`.
 *
 * One generic helper plus one wrapper per backend handler. The wrappers exist so
 * a caller never spells a method name or a parameter key by hand: those are the
 * contract, and a typo in either is a runtime error nothing catches.
 *
 * **Wire shape.** `params` keys are the Rust handler's argument names, in
 * snake_case (see `ipc/rpc.ts` — they are forwarded verbatim inside the opaque
 * `params` object, so Tauri's camelCase conversion never touches them). The
 * RESULTS are snake_case too: `garrulus-be`'s report types derive plain `serde`
 * without a `rename_all`, so `display_name` and `note_count` arrive spelled
 * exactly as Rust spells them. The interfaces below mirror that rather than
 * pretending otherwise — a single renaming layer would have to be applied on
 * every read and would silently drop whatever it forgot. The one exception is
 * `RemoteConfig`, which is camelCase because it is also the persisted shape of a
 * registry entry — see the Remote section.
 *
 * **When the backend is down** every call rejects with `BackendNotRunning`. That
 * is a real state (a vault the shell cannot reach), not something to paper over:
 * surface it, do not substitute an empty vault.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { garrulus } from './rpc';

export { garrulus };

// ── Vault ────────────────────────────────────────────────────────────────────

/** What the shell gets back when a vault opens — enough to title the window and
 *  draw the empty state, never the vault's contents. */
export interface VaultSummary {
  /** Stable id, also the key of this vault's index cache directory. */
  id: string;
  /** Absolute vault root. */
  root: string;
  /** Name shown in the switcher — the folder name unless renamed. */
  display_name: string;
  /** Notes indexed at open. */
  note_count: number;
  /** Note types declared under `<vault>/.arbor/garrulus/types/`. */
  type_count: number;
}

/** One vault the active profile knows about (`garrulus/vaults.json`). */
export interface VaultEntry {
  id: string;
  /** Absolute path of the vault root. */
  path: string;
  display_name: string;
  /** Configured sync destination, or `null` for a local-only vault. Defined in
   *  the Remote section below — it lives on the registry entry, not in the
   *  vault, because a mirror path is machine-specific. */
  remote?: RemoteConfig | null;
  /** Unix milliseconds of the last successful open. */
  last_opened?: number | null;
}

/** Open an existing vault: parse it, build the index, start the watcher. */
export const openVault = (path: string): Promise<VaultSummary> =>
  garrulus<VaultSummary>('garrulus_open_vault', { path });

/** Create a vault at `path` (marker folder, default settings, built-in types)
 *  and open it. Fails when a vault is already there — that is `openVault`. */
export const createVault = (path: string, displayName?: string): Promise<VaultSummary> =>
  garrulus<VaultSummary>('garrulus_create_vault', { path, display_name: displayName ?? null });

/** Every known vault, most recently opened first. */
export const listVaults = (): Promise<VaultEntry[]> =>
  garrulus<VaultEntry[]>('garrulus_list_vaults');

/** Close the open vault: stop the watcher, drop the index, detach the remote. */
export const closeVault = (): Promise<void> => garrulus<void>('garrulus_close_vault');

/** Re-read every note and rebuild the index from scratch, resolving the note
 *  count. The escape hatch for the one thing a cache cannot promise — that it
 *  never drifted (a `git checkout` in a terminal, a network share, Obsidian
 *  mid-crash). Same rebuild an open does, so it is not cheap: offer it, do not
 *  call it on a timer. */
export const rebuildIndex = (): Promise<number> => garrulus<number>('garrulus_rebuild_index');

// ── Notes ────────────────────────────────────────────────────────────────────

/** A note as bytes. The source is the record; everything else is derived. */
export interface NoteSource {
  /** Vault-relative path. */
  path: string;
  /** The file's whole text, frontmatter included. */
  text: string;
}

/** Read one note's source. */
export const readNote = (path: string): Promise<NoteSource> =>
  garrulus<NoteSource>('garrulus_read_note', { path });

/** Write a note and re-index it. What is passed is what lands on disk. */
export const writeNote = (path: string, text: string): Promise<void> =>
  garrulus<void>('garrulus_write_note', { path, text });

/** Create a note, optionally with a starting body (a rendered template). */
export const createNote = (path: string, text?: string): Promise<NoteSource> =>
  garrulus<NoteSource>('garrulus_create_note', { path, text: text ?? null });

/** Move/rename a note inside the vault. */
export const renameNote = (path: string, newPath: string): Promise<void> =>
  garrulus<void>('garrulus_rename_note', { path, new_path: newPath });

/** Delete a note — to `<vault>/.arbor/garrulus/trash/`, never to nowhere. */
export const deleteNote = (path: string): Promise<void> =>
  garrulus<void>('garrulus_delete_note', { path });

// ── Trash ────────────────────────────────────────────────────────────────────

/** One note waiting in the vault's trash. Enough to list it without opening
 *  anything — the body stays in the trash folder until it is restored. */
export interface TrashedNote {
  /** Opaque id, the key of every other trash call. */
  id: string;
  /** Where the note was when it was deleted, and where a restore puts it back. */
  original: string;
  /** When it was trashed, as the timestamp string the backend recorded. */
  trashed_at: string;
  /** The note's title at the moment of the delete. */
  title: string;
}

/** Everything currently in the trash. Debris from an interrupted delete is
 *  skipped by the backend rather than listed — there is nothing to do about it. */
export const trashList = (): Promise<TrashedNote[]> =>
  garrulus<TrashedNote[]>('garrulus_trash_list');

/** Put a trashed note back where it came from, and back into the index. Rejects
 *  when something is already at the original path — restoring over a note
 *  written since the delete would lose the newer one. */
export const trashRestore = (id: string): Promise<void> =>
  garrulus<void>('garrulus_trash_restore', { id });

/** Drop one entry from the trash. Still not a hard delete: the files go to the
 *  OS trash, so even this confirmed second delete is recoverable outside Arbor. */
export const trashPurge = (id: string): Promise<void> =>
  garrulus<void>('garrulus_trash_purge', { id });

/** Empty the trash. Purges entry by entry, so one failure does not strand the
 *  rest — reload the list rather than assuming it is now empty. */
export const trashEmpty = (): Promise<void> => garrulus<void>('garrulus_trash_empty');

// ── Note types & templates ───────────────────────────────────────────────────

/** How a type's field is edited, filtered and sorted. */
export type FieldKind =
  | 'text' | 'number' | 'bool' | 'date' | 'enum' | 'tags' | 'link' | 'code_link';

/** One frontmatter field a note type declares. */
export interface FieldSpec {
  /** The frontmatter key (lowercase, no spaces — it is YAML). */
  key: string;
  /** What the form calls it, in the user's language. */
  label: string;
  kind: FieldKind;
  /** Options for an `enum` field. **Empty means open**: the dropdown offers the
   *  values already used in the vault and accepts a new one. */
  values: string[];
  /** Prefilled when a note of this type is created. */
  default?: string | null;
  /** Written into the frontmatter even when empty, so the gap is visible. */
  required: boolean;
  /** This field groups the board view's columns (at most one per type). */
  board: boolean;
}

/** Which panels open with a note of this type. */
export interface NoteLayout {
  /** Panel ids, in the order the type asked for. Rendered as declared. */
  panels: string[];
  /** Give the editor the width and put the side panels away. */
  wide_editor: boolean;
}

/** A note type: where its notes land, what they are called, what they contain. */
export interface NoteType {
  id: string;
  name: string;
  /** A lucide icon name. */
  icon: string;
  /** The type's colour — used for kind and state, never for decoration. */
  accent: string;
  /** Where new notes of this type land. `''` is the vault root. */
  folder: string;
  /** Filename pattern, expanded from the template context. */
  naming: string;
  /** Glob over the note's vault-relative path — the second recognition rule. */
  match_folder?: string | null;
  /** The body a new note of this type starts with. */
  template: string;
  /** Frontmatter pairs identifying the type — the first recognition rule. */
  match_frontmatter: Record<string, string>;
  layout: NoteLayout;
  fields: FieldSpec[];
}

/** A note the shell could create: where it would go, what it would contain. */
export interface RenderedNote {
  type_id: string;
  /** Proposed vault-relative path, from the type's folder + naming pattern. */
  path: string;
  /** The rendered body, `{{cursor}}` included — the editor consumes the marker. */
  text: string;
}

/** Every note type declared in the open vault. */
export const listTypes = (): Promise<NoteType[]> => garrulus<NoteType[]>('garrulus_list_types');

/** Tag an existing note as being of a type (sets its frontmatter `type`). */
export const applyType = (path: string, typeId: string): Promise<void> =>
  garrulus<void>('garrulus_apply_type', { path, type_id: typeId });

/** Preview a new note of a type. Writes nothing — `createNote` does that. */
export const renderTemplate = (typeId: string, title: string): Promise<RenderedNote> =>
  garrulus<RenderedNote>('garrulus_render_template', { type_id: typeId, title });

// ── Search, links & problems ─────────────────────────────────────────────────

/** A half-open byte range inside a rendered string. */
export interface MatchRange {
  start: number;
  end: number;
}

/** A body excerpt around a match, ready to render. */
export interface Snippet {
  /** The excerpt, with an ellipsis where it was cut. */
  text: string;
  /** Ranges to highlight, as byte offsets **into `text`**. */
  ranges: MatchRange[];
}

/** One search or quick-switch result. */
export interface Hit {
  /** The note's id (its vault-relative path, or a frontmatter uid). */
  id: string;
  title: string;
  /** Higher is better, and only comparable within one result list. */
  score: number;
  /** Ranges in `title` the query matched — what the UI underlines. */
  title_matches: MatchRange[];
  snippet?: Snippet | null;
}

/** A link pointing at the note whose panel this appears in. */
export interface Backlink {
  /** The note that links here. */
  from: string;
  /** The note being linked. */
  to: string;
  heading?: string | null;
  alias?: string | null;
  /** `true` for `![[embed]]` transclusions. */
  embed: boolean;
}

/** A note naming another note's title without linking to it. */
export interface Mention {
  from: string;
  to: string;
  snippet: Snippet;
}

/** A `[[Foo]]` with no `Foo` — first-class: that is how a note gets created. */
export interface UnresolvedLink {
  from: string;
  /** The target exactly as written. */
  target: string;
  heading?: string | null;
  embed: boolean;
}

/** A note's incoming edges. */
export interface Backlinks {
  path: string;
  backlinks: Backlink[];
  /** One click from becoming links, which is why they are surfaced. */
  unlinked_mentions: Mention[];
}

/** What is wrong with the vault, as far as the link graph can tell. */
export interface VaultProblems {
  unresolved: UnresolvedLink[];
  /** Notes nothing links to. */
  orphans: string[];
}

/** Full-text + structured search: `type:bug stato:aperto free text`. An empty
 *  query returns nothing rather than everything. */
export const search = (query: string): Promise<Hit[]> => garrulus<Hit[]>('garrulus_search', { query });

/** Fuzzy title match for the quick switcher, best first. */
export const quickSwitch = (query: string, limit?: number): Promise<Hit[]> =>
  garrulus<Hit[]>('garrulus_quick_switch', { query, limit: limit ?? null });

/** Everything pointing at one note, plus the mentions that do not yet. */
export const backlinks = (path: string): Promise<Backlinks> =>
  garrulus<Backlinks>('garrulus_backlinks', { path });

/** Broken links and orphans across the vault. */
export const problems = (): Promise<VaultProblems> => garrulus<VaultProblems>('garrulus_problems');

// ── Remote ───────────────────────────────────────────────────────────────────

/**
 * Where a vault syncs to.
 *
 * **This section is camelCase on the wire**, unlike the rest of the file:
 * `RemoteConfig` is persisted in `garrulus/vaults.json` and derives
 * `rename_all = "camelCase"` there, so `gitRemote` really is spelled that way.
 * The types around it (`RemoteDescriptor`, `RemoteCapabilities`) derive nothing
 * and stay snake_case — `atomic_batch` is not a typo either.
 */

/** Which implementation is behind a destination. */
export type RemoteKind = 'git' | 'folder';

/** A vault's configured sync destination, as stored on its registry entry.
 *
 *  Flat rather than a tagged union so a half-filled settings form round-trips:
 *  the fields of the kind that is not selected are simply absent. Which ones
 *  matter is decided by `kind`. */
export interface RemoteConfig {
  kind: RemoteKind;
  /** Git only: the remote's name. Absent means `origin`. */
  gitRemote?: string | null;
  /** Git only: the branch to track. Absent means "whatever is checked out". */
  branch?: string | null;
  /** Folder only: the **absolute** path of the mirror directory. Machine-local
   *  by nature — this is the field that must never travel to the other PC. */
  folder?: string | null;
}

/** What a remote can actually do, so the UI never offers what it cannot. */
export interface RemoteCapabilities {
  /** Can it answer `noteHistory` / `revision`? A folder mirror cannot, and the
   *  history panel is hidden rather than shown broken. */
  history: boolean;
  /** Is a push all-or-nothing? */
  atomic_batch: boolean;
  /** Can it detect concurrent edits, or is it last-writer-wins? */
  conflicts: boolean;
}

/** Identity of the installed destination — what the sync dropdown names. */
export interface RemoteDescriptor {
  /** Stable id: the git remote name, or the mirror path. */
  id: string;
  kind: RemoteKind;
  /** What the user sees. */
  display: string;
  capabilities: RemoteCapabilities;
}

/** Identity **and** standing, in one shape. Returned as a unit so the title-bar
 *  button can be drawn from one round trip: asking for the descriptor and the
 *  state separately would let the two disagree. */
export interface RemoteStatus {
  descriptor: RemoteDescriptor;
  state: SyncState;
}

/** Point the open vault at a destination: persist it, install it, probe it.
 *  Persisted before it is probed — a destination that did not answer is still
 *  the one the user configured, and the returned state says it is offline. */
export const setRemote = (config: RemoteConfig): Promise<RemoteStatus> =>
  garrulus<RemoteStatus>('garrulus_set_remote', { config });

/** Make the vault local-only again. Stops syncing; changes no file — a git
 *  vault keeps its `.git`, a mirrored vault keeps its mirror. */
export const clearRemote = (): Promise<void> => garrulus<void>('garrulus_clear_remote');

/** Tell the backend whether the Garrulus window has focus.
 *
 *  Load-bearing, not a nicety: a headless backend has no window to ask, so the
 *  "only probe while focused" preference is a setting that silently does nothing
 *  until the window pushes this. Call it from the window's focus and blur
 *  handlers. */
export const setFocus = (focused: boolean): Promise<void> =>
  garrulus<void>('garrulus_set_focus', { focused });

/** The destination recorded for the open vault, read from the registry rather
 *  than from the live remote: one that failed to build at open time must still
 *  show in the settings panel instead of inviting a retype. */
export const remoteConfig = (): Promise<RemoteConfig | null> =>
  garrulus<RemoteConfig | null>('garrulus_remote_config');

/** Try a destination without adopting it — the settings panel's "test" button.
 *  Persists nothing, and rejects with the reason when it does not work: "does
 *  this work?" is the question, so the answer has to carry the failure. */
export const testRemote = (config: RemoteConfig): Promise<RemoteStatus> =>
  garrulus<RemoteStatus>('garrulus_test_remote', { config });

/** Create a **private** repository through the shell's git provider, point the
 *  vault's `origin` at it, and adopt it. There is no public option at any layer:
 *  a personal note vault has no business being public. */
export const createRemoteRepo = (name: string): Promise<RemoteConfig> =>
  garrulus<RemoteConfig>('garrulus_create_remote_repo', { name });

// ── Sync ─────────────────────────────────────────────────────────────────────

/**
 * The state of the vault against its remote — the one thing the sync button
 * shows.
 *
 * Externally tagged, so a unit state is the bare string and a state carrying a
 * count is a single-key object: `'synced'`, `{ 'has-changes': 3 }`,
 * `{ diverged: { ahead: 3, behind: 2 } }`, `'no-remote'`.
 */
export type SyncState =
  | 'synced'
  | 'offline'
  | 'no-remote'
  | { 'has-changes': number }
  | { behind: number }
  | { ahead: number }
  | { conflict: number }
  | { diverged: { ahead: number; behind: number } };

/** The kebab-case tag of a `SyncState`, whichever shape it arrived in — what the
 *  button keys its icon, colour and label off. */
export function syncStateTag(state: SyncState): string {
  return typeof state === 'string' ? state : Object.keys(state)[0];
}

/** The count a `SyncState` carries, or 0 for the unit states. `diverged` has two
 *  and reports neither — read it off the state itself. */
export function syncStateCount(state: SyncState): number {
  if (typeof state === 'string') return 0;
  const value = Object.values(state)[0];
  return typeof value === 'number' ? value : 0;
}

/** One note two machines disagree about. The three sides travel as text: the UI
 *  never sees a merge marker, and the local text stayed in the file. */
export interface Conflict {
  /** The note, vault-relative. It still holds the local version. */
  path: string;
  /** The common ancestor, when the remote has one. */
  base?: string | null;
  local: string;
  remote: string;
  /** Where the remote text was parked beside the note. */
  side_file?: string | null;
}

/** What a pull did. Conflicts are a normal outcome, not an error path. */
export interface PullOutcome {
  applied: string[];
  conflicts: Conflict[];
}

/** What a full sync did — for the button's toast and the log. */
export interface SyncReport {
  applied: number;
  conflicts: number;
  /** The push half is skipped when the pull conflicted. */
  pushed: boolean;
}

/** One past version of a note. */
export interface Revision {
  /** Opaque id, to be fed back when asking for that revision's text. */
  id: string;
  /** Who wrote it — for Garrulus's own commits, the device. */
  author: string;
  /** Unix seconds. */
  timestamp: number;
  summary: string;
}

/** How a conflict was settled. "Merge by hand" is not one of these: the user
 *  edits the note and then resolves as `mine`. */
export type ConflictResolution = 'mine' | 'theirs';

/** The current state against the remote. The only sync call safe to run
 *  unattended — it fetches and reports, and cannot change a byte. */
export const syncState = (): Promise<SyncState> => garrulus<SyncState>('garrulus_sync_state');

/** Commit, pull, push — the sync button's main action. */
export const syncNow = (message?: string): Promise<SyncReport> =>
  garrulus<SyncReport>('garrulus_sync_now', { message: message ?? null });

/** Pull only. */
export const pull = (): Promise<PullOutcome> => garrulus<PullOutcome>('garrulus_pull');

/** Push only. An empty `notes` list means "everything the remote considers
 *  changed", which is what the button sends. */
export const push = (notes: string[] = [], message?: string): Promise<void> =>
  garrulus<void>('garrulus_push', { notes, message: message ?? null });

/** The conflicts this session's last pull produced. */
export const conflicts = (): Promise<Conflict[]> => garrulus<Conflict[]>('garrulus_conflicts');

/** Settle one conflict and drop its side file. */
export const resolveConflict = (
  path: string,
  sideFile: string,
  resolution: ConflictResolution,
): Promise<void> =>
  garrulus<void>('garrulus_resolve_conflict', { path, side_file: sideFile, resolution });

/** A note's revisions, newest first. Empty when the remote keeps no history —
 *  the panel is hidden in that case rather than shown broken. */
export const noteHistory = (path: string): Promise<Revision[]> =>
  garrulus<Revision[]>('garrulus_note_history', { path });

/** One note's text as of a past revision — the history panel's preview, and the
 *  source of "restore this version" (which writes through `writeNote`, so the
 *  restore is an ordinary edit the user can undo). `rev` is a `Revision.id`.
 *  Rejects on a remote whose `capabilities.history` is false. */
export const revision = (path: string, rev: string): Promise<string> =>
  garrulus<string>('garrulus_revision', { path, rev });

// ── Self-test ────────────────────────────────────────────────────────────────

/** Round-trip the framed-stdio handshake. Resolves `'pong'`. */
export const bePing = (): Promise<string> => garrulus<string>('be_ping');

/** Echo — proves argument decode across the boundary. */
export const beEcho = (message: string): Promise<string> =>
  garrulus<string>('be_echo', { message });

// ── Events ───────────────────────────────────────────────────────────────────

/** Payload of `garrulus:vault-changed`: the vault that moved, and which notes.
 *  The list is capped by the backend — past the cap, reload the tree wholesale. */
export interface VaultChanged {
  root: string;
  paths: string[];
  /** The burst exceeded the watcher's per-event cap, so `paths` is a sample
   *  rather than the whole story — reload the tree wholesale instead of patching
   *  the listed entries. */
  truncated: boolean;
}

/** Something changed the vault on disk (the other editor, a pull, another
 *  process). Debounced by the backend's watcher, so this is one event per burst. */
export const onVaultChanged = (cb: (e: VaultChanged) => void): Promise<UnlistenFn> =>
  listen<VaultChanged>('garrulus:vault-changed', (e) => cb(e.payload));

/** Payload of `garrulus:sync-state`: where the vault stands, and whether this
 *  tick is worth telling the user about. */
export interface SyncStateEvent {
  state: SyncState;
  /**
   * Connectivity news, or `null` — which is the usual case.
   *
   * `'lost'` arrives once per outage (never on the first missed probe: one
   * missed probe is a dropped packet), `'regained'` only for someone who was
   * told about the loss. The backend already applies that gating, so show the
   * toast whenever this is non-null and never synthesise one from `state`.
   */
  toast: 'lost' | 'regained' | null;
}

/** The background probe reported. Read-only by construction — it fetches and
 *  compares, and cannot commit, pull or push; everything that changes a byte
 *  happens because the user pressed the button. Emitted only when the state
 *  changed or a toast is due, so it is safe to render on directly. */
export const onSyncState = (cb: (e: SyncStateEvent) => void): Promise<UnlistenFn> =>
  listen<SyncStateEvent>('garrulus:sync-state', (e) => cb(e.payload));

/** `garrulus-be` finished attaching to the router. The window's one-shot loads
 *  race the spawn, so anything read at mount is re-read here. */
export const onGarrulusBeUp = (cb: () => void): Promise<UnlistenFn> =>
  listen('arbor://garrulus-be-up', () => cb());

/** `garrulus-be` died (crash / kill). The vault is unreachable until the window
 *  is re-opened, which respawns it. */
export const onGarrulusBeDown = (cb: () => void): Promise<UnlistenFn> =>
  listen('arbor://garrulus-be-down', () => cb());
