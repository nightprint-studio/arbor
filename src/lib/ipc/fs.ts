import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { platform } from './rpc';

export interface FsEntry {
  name:     string;
  path:     string;
  is_dir:   boolean;
  size:     number | null;
  modified: number | null;  // Unix timestamp ms
  created:  number | null;  // Unix timestamp ms (null when the FS has no birth time)
}

export interface FsRoot {
  name: string;
  path: string;
  kind: 'home' | 'desktop' | 'documents' | 'downloads' | 'drive' | 'wsl';
}

/** Read a directory — returns entries with metadata. Dot-prefixed entries
 *  are skipped unless `showHidden` is set. */
export const fsReadDir = (path: string, showHidden = false) =>
  platform<FsEntry[]>('fs_read_dir', { path, show_hidden: showHidden });

/** Return quick-access roots (common dirs + drives). */
export const listFsRoots = () =>
  platform<FsRoot[]>('list_fs_roots');

/** Installed WSL distributions as `\\wsl.localhost\<distro>` roots (Windows;
 *  empty elsewhere or when WSL isn't installed). */
export const listWslDistros = () =>
  platform<FsRoot[]>('list_wsl_distros');

export const fsCreateDir      = (path: string)                      => platform<void>('fs_create_dir',        { path });
export const fsCreateFile     = (path: string)                      => platform<void>('fs_create_file',       { path });
export const fsWriteTextFile  = (path: string, content: string)     => platform<void>('fs_write_text_file',   { path, content });
export const fsReadTextFile   = (path: string)                      => platform<string>('fs_read_text_file', { path });
export const fsRename         = (oldPath: string, newPath: string)  => platform<void>('fs_rename',            { old_path: oldPath, new_path: newPath });
export const fsDelete         = (path: string)                      => platform<void>('fs_delete',            { path });

// ── File explorer: copy / move / delete / open / watch ─────────────────────
/** Copy entries into `destDir`; returns the created destination paths. With
 *  `overwrite`, same-named items merge into / replace the existing entry
 *  instead of getting a " (2)" suffix. */
export const fsCopy        = (sources: string[], destDir: string, overwrite = false, opId?: string) => invoke<string[]>('fs_copy', { sources, destDir, overwrite, opId: opId ?? null });
/** Move (cut+paste) entries into `destDir`; returns the new paths. With
 *  `overwrite`, same-named items merge into / replace the existing entry. */
export const fsMove        = (sources: string[], destDir: string, overwrite = false, opId?: string) => invoke<string[]>('fs_move', { sources, destDir, overwrite, opId: opId ?? null });
/** Duplicate entries in place ("file (2).ext"). Returns the created paths. */
export const fsDuplicate   = (paths: string[], opId?: string) => invoke<string[]>('fs_duplicate', { paths, opId: opId ?? null });
/** Request cancellation of a running copy/move/duplicate by its op id. */
export const fsCancelOp    = (opId: string) => invoke<void>('fs_cancel_op', { opId });
/** One old→new rename pair for a batch rename. */
export interface RenamePair { from: string; to: string; }
/** Batch-rename many entries atomically (two-phase, collision-safe). */
export const fsRenameMany  = (pairs: RenamePair[]) => platform<string[]>('fs_rename_many', { pairs });

/** Recursive size of a folder (bytes + file/dir counts). */
export interface DirSize { bytes: number; files: number; dirs: number; }
export const fsDirSize     = (path: string) => platform<DirSize>('fs_dir_size', { path });
/** Combined recursive size of several paths (multi-selection footer). */
export const fsPathsSize   = (paths: string[]) => platform<DirSize>('fs_paths_size', { paths });

/** Per-drive storage usage for the Overview dashboard. */
export interface DriveUsage { name: string; path: string; total: number | null; free: number | null; }
export interface OverviewStats { drives: DriveUsage[]; total_capacity: number; total_free: number; }
export const fsOverviewStats = () => platform<OverviewStats>('fs_overview_stats');

/** One item in the Recycle Bin / trash. */
export interface TrashEntry { id: string; name: string; original_path: string; deleted_at: number | null; }
/** List trash items (newest first). Empty on macOS. */
export const fsTrashList    = () => platform<TrashEntry[]>('fs_trash_list');
/** Restore trashed items to their original location (Windows/Linux). */
export const fsTrashRestore = (ids: string[]) => platform<void>('fs_trash_restore', { ids });
/** Permanently delete trashed items (Windows/Linux). */
export const fsTrashPurge   = (ids: string[]) => platform<void>('fs_trash_purge', { ids });
/** Empty the whole Recycle Bin (Windows/Linux). */
export const fsTrashEmpty   = () => platform<void>('fs_trash_empty');

/** Progress event for a long-running file operation (copy/move/duplicate). */
export interface FsOpProgress {
  op_id: string;
  kind: 'copy' | 'move' | 'duplicate';
  done_files: number;
  total_files: number;
  done_bytes: number;
  total_bytes: number;
  current: string;
}
/** Subscribe to this window's file-operation progress events. */
export const onFsOpProgress = (cb: (p: FsOpProgress) => void): Promise<UnlistenFn> =>
  listen<FsOpProgress>('arbor://fs-op-progress', e => cb(e.payload));
/** Move entries to the OS trash / Recycle Bin (recoverable). */
export const fsTrash       = (paths: string[]) => platform<void>('fs_trash', { paths });
/** Restore previously-trashed entries to their original locations (undo of
 *  `fsTrash`). Windows / Linux only. */
export const fsUntrash     = (paths: string[]) => platform<void>('fs_untrash', { paths });
/** Permanently delete entries from disk (Shift+Delete). */
export const fsDeleteMany  = (paths: string[]) => platform<void>('fs_delete_many', { paths });
/** Recursively search `root` for entries whose name matches `query` (glob when
 *  it contains `*`/`?`, else case-insensitive substring). Capped at `limit`
 *  (default 5000). Each result carries its full path. */
export const fsSearch = (root: string, query: string, showHidden = false, limit?: number) =>
  platform<FsEntry[]>('fs_search', { root, query, show_hidden: showHidden, limit });
/** Compress `sources` into a new ZIP named `archiveName` inside `destDir`
 *  (collision-resolved). Returns the created archive path. */
export const fsZip = (sources: string[], destDir: string, archiveName: string) =>
  platform<string>('fs_zip', { sources, dest_dir: destDir, archive_name: archiveName });
/** Extract a ZIP `archive`, into `destDir` or — when omitted — a new sibling
 *  folder named after the archive. Returns the destination folder path. */
export const fsUnzip = (archive: string, destDir?: string) =>
  platform<string>('fs_unzip', destDir ? { archive, dest_dir: destDir } : { archive });
/** Set an image file as the desktop wallpaper (Windows / macOS / GNOME). */
export const fsSetWallpaper = (path: string) => invoke<void>('fs_set_wallpaper', { path });
/** Open a path with the OS default app (file) or file manager (dir). */
export const fsOpenDefault = (path: string) => invoke<void>('fs_open_default', { path });
/** Reveal a path in the OS file manager, selecting it. */
export const fsRevealInDir = (path: string) => invoke<void>('fs_reveal_in_dir', { path });
/** Open the OS terminal rooted at `path` (the folder, or a file's parent),
 *  detached so it outlives Arbor. Windows Terminal / cmd · Terminal.app ·
 *  the first available Linux terminal emulator. */
export const fsOpenTerminal = (path: string) => invoke<void>('fs_open_terminal', { path });
/** Expand `%VAR%` / `$VAR` / leading `~` in a typed path. The virtual names
 *  `appdata` / `localappdata` / `home` resolve cross-platform, so `%appdata%`
 *  works on every OS. Returns the input unchanged when there's nothing to expand. */
export const fsExpandPath = (path: string) => platform<string>('fs_expand_path', { path });
/** Open the built-in explorer window at a path (focusing/reusing it per the
 *  one-window setting). `reveal = true` selects the file inside its folder;
 *  `reveal = false` just opens the folder. Used when the user routes the app's
 *  "Open / Reveal in File Explorer" actions to the built-in explorer. */
export const revealInExplorerWindow = (path: string, reveal: boolean) =>
  invoke<void>('reveal_in_explorer', { path, reveal });
/** A pending reveal handed to a freshly-opened explorer window. */
export interface ExplorerRevealPayload { dir: string; select: string | null; }
/** Drain the pending reveal for a window label (explorer window, on mount). */
export const takeExplorerReveal = (label: string) =>
  invoke<ExplorerRevealPayload | null>('take_explorer_reveal', { label });

// ── Cross-window clipboard (copy / cut / paste between explorer windows) ─────
export type ClipOp = 'copy' | 'cut';
/** The shared explorer clipboard payload (process-wide, mirrored per window). */
export interface ClipData { op: ClipOp; paths: string[]; }
/** Set the shared clipboard; broadcasts `arbor://explorer-clip-changed`. */
export const explorerClipSet = (op: ClipOp, paths: string[]) =>
  invoke<void>('explorer_clip_set', { op, paths });
/** Read the shared clipboard (seed a window's local mirror on mount). */
export const explorerClipGet = () => invoke<ClipData | null>('explorer_clip_get');
/** Clear the shared clipboard (after a cut→paste move); broadcasts the change. */
export const explorerClipClear = () => invoke<void>('explorer_clip_clear');

// ── Cross-window drag & drop (overlay ghost + drop hit-testing) ──────────────
/** Ensure the shared drag-ghost overlay window exists (built once, reused). */
export const ensureDragOverlay = () => invoke<void>('ensure_drag_overlay');
/** Show the overlay with `text` at logical screen coordinates `x`/`y`. */
export const dragOverlayShow = (text: string, x: number, y: number) =>
  invoke<void>('drag_overlay_show', { text, x, y });
/** Move the overlay to logical screen coordinates `x`/`y`. */
export const dragOverlayMove = (x: number, y: number) =>
  invoke<void>('drag_overlay_move', { x, y });
/** Hide the overlay (drag ended, or cursor re-entered the source window). */
export const dragOverlayHide = () => invoke<void>('drag_overlay_hide');
/** Drain the current overlay label (overlay window pulls this on mount). */
export const getDragOverlayText = () => invoke<string>('get_drag_overlay_text');
/** On drop, hand the dragged paths to another explorer window under the cursor
 *  (logical screen coords). Returns true when a target window was notified. */
export const explorerDropDispatch = (sourceLabel: string, x: number, y: number, paths: string[]) =>
  invoke<boolean>('explorer_drop_dispatch', { sourceLabel, x, y, paths });
/** Open the OS-native Properties dialog for a path (Windows property sheet /
 *  macOS Finder Get Info / Linux FileManager1 D-Bus). */
export const fsShowProperties = (path: string) => invoke<void>('fs_show_properties', { path });
/** Native system icon for a query (a file extension like ".rs", or an absolute
 *  path — `.exe` yields its embedded icon), as a `data:image/png;base64,…` URI. */
export const fsIcon = (query: string, size: number) => invoke<string>('fs_icon', { query, size });
/** Start watching `path` for changes (replaces any prior watch). Emits the
 *  `arbor://fs-changed` Tauri event on any change in the directory. Pass
 *  `recursive = true` to also catch changes in sub-folders (e.g. a project tree);
 *  the flat explorer leaves it false. */
export const fsWatchStart  = (path: string, recursive = false) =>
  invoke<void>('fs_watch_start', { path, recursive });
/** Stop the active filesystem watch. */
export const fsWatchStop   = () => invoke<void>('fs_watch_stop');
/** Subscribe to this window's `arbor://fs-changed` signal (fired when the watched
 *  directory changes; carries no payload — re-read what you care about). */
export const onFsChanged   = (cb: () => void): Promise<UnlistenFn> =>
  listen('arbor://fs-changed', () => cb());

// ── File explorer: git awareness (TortoiseGit-style overlays + actions) ─────
/** Overlay badge for one explorer entry (or a rolled-up folder). */
export type GitBadge =
  | 'conflicted' | 'modified' | 'deleted' | 'renamed' | 'added' | 'untracked' | 'ignored';

/** Lightweight marker for a child folder that is itself a git repo root. */
export interface GitRepoMarker {
  branch: string | null;
  detached: boolean;
}

export interface FsGitStatus {
  in_repo: boolean;
  repo_root: string | null;
  branch: string | null;
  detached: boolean;
  ahead: number;
  behind: number;
  /** Map keyed by `normPath(entry.path)` → badge. Only non-clean entries present. */
  badges: Record<string, GitBadge>;
  /** Map keyed by `normPath(entry.path)` → marker, for immediate child folders
   *  that are themselves git repo roots (flagged even when `dir` isn't a repo). */
  repos: Record<string, GitRepoMarker>;
}

/** Git status for `dir`'s entries (badges + branch / ahead-behind). Cached per
 *  repo-root; pass `refresh = true` (off the fs watcher) to recompute. */
export const fsGitStatus = (dir: string, refresh = false) =>
  invoke<FsGitStatus>('fs_git_status', { dir, refresh });
/** Stage paths (files / folders / deletions) in their enclosing repo. */
export const fsGitStage   = (paths: string[]) => invoke<void>('fs_git_stage',   { paths });
/** Unstage paths (reset to HEAD). */
export const fsGitUnstage = (paths: string[]) => invoke<void>('fs_git_unstage', { paths });
/** Discard working-tree changes for paths (snapshots to Recovery first). */
export const fsGitDiscard = (paths: string[]) => invoke<void>('fs_git_discard', { paths });
/** Append paths to the repo's `.gitignore` (anchored, folders get a trailing slash). */
export const fsGitIgnore  = (paths: string[]) => invoke<void>('fs_git_ignore',  { paths });
/** Bring the main Arbor window forward and open the repo containing `path`
 *  (delegates the heavy git operations — diff / log / blame — to Arbor's UI). */
export const fsOpenInArbor = (path: string) => invoke<void>('fs_open_in_arbor', { path });

/** One changed file in the staged or unstaged list. */
export interface GitChange {
  /** Absolute path (native separators) — matches `fsReadDir` entry paths. */
  path: string;
  /** Repo-relative path (forward slashes) for display. */
  rel: string;
  badge: GitBadge;
}
/** Working-tree change list for the repo enclosing `dir`. A file staged then
 *  edited again appears in BOTH lists, like `git status`. */
export interface GitChanges {
  repo_root: string | null;
  branch: string | null;
  staged: GitChange[];
  unstaged: GitChange[];
}
/** Full staged/unstaged change list for the repo enclosing `dir`. */
export const fsGitChanges = (dir: string) => invoke<GitChanges>('fs_git_changes', { dir });

/** One local branch of a repo. */
export interface FsBranch { name: string; is_head: boolean; }
/** Local branches of the repo enclosing `path` (sorted, case-insensitive). */
export const fsGitBranches = (path: string) => invoke<FsBranch[]>('fs_git_branches', { path });
/** Switch the repo enclosing `path` to `branch` (safe checkout — fails on
 *  conflicting uncommitted changes). */
export const fsGitCheckout = (path: string, branch: string) => invoke<void>('fs_git_checkout', { path, branch });
/** Remote URL (origin, else first remote) of the repo enclosing `path`, or
 *  `null` when there's no repo / no remote. Used to build "Copy project link". */
export const fsGitRemoteUrl = (path: string) => invoke<string | null>('fs_git_remote_url', { path });
