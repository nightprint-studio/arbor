import { invoke } from '@tauri-apps/api/core';

export interface FsEntry {
  name:     string;
  path:     string;
  is_dir:   boolean;
  size:     number | null;
  modified: number | null;  // Unix timestamp ms
}

export interface FsRoot {
  name: string;
  path: string;
  kind: 'home' | 'desktop' | 'documents' | 'downloads' | 'drive';
}

/** Read a directory — returns entries with metadata. Dot-prefixed entries
 *  are skipped unless `showHidden` is set. */
export const fsReadDir = (path: string, showHidden = false) =>
  invoke<FsEntry[]>('fs_read_dir', { path, showHidden });

/** Return quick-access roots (common dirs + drives). */
export const listFsRoots = () =>
  invoke<FsRoot[]>('list_fs_roots');

export const fsCreateDir      = (path: string)                      => invoke<void>('fs_create_dir',        { path });
export const fsCreateFile     = (path: string)                      => invoke<void>('fs_create_file',       { path });
export const fsWriteTextFile  = (path: string, content: string)     => invoke<void>('fs_write_text_file',   { path, content });
export const fsReadTextFile   = (path: string)                      => invoke<string>('fs_read_text_file', { path });
export const fsRename         = (oldPath: string, newPath: string)  => invoke<void>('fs_rename',            { oldPath, newPath });
export const fsDelete         = (path: string)                      => invoke<void>('fs_delete',            { path });

// ── File explorer: copy / move / delete / open / watch ─────────────────────
/** Copy entries into `destDir`; returns the created destination paths. */
export const fsCopy        = (sources: string[], destDir: string) => invoke<string[]>('fs_copy', { sources, destDir });
/** Move (cut+paste) entries into `destDir`; returns the new paths. */
export const fsMove        = (sources: string[], destDir: string) => invoke<string[]>('fs_move', { sources, destDir });
/** Move entries to the OS trash / Recycle Bin (recoverable). */
export const fsTrash       = (paths: string[]) => invoke<void>('fs_trash', { paths });
/** Permanently delete entries from disk (Shift+Delete). */
export const fsDeleteMany  = (paths: string[]) => invoke<void>('fs_delete_many', { paths });
/** Recursively search `root` for entries whose name matches `query` (glob when
 *  it contains `*`/`?`, else case-insensitive substring). Capped at `limit`
 *  (default 5000). Each result carries its full path. */
export const fsSearch = (root: string, query: string, showHidden = false, limit?: number) =>
  invoke<FsEntry[]>('fs_search', { root, query, showHidden, limit });
/** Compress `sources` into a new ZIP named `archiveName` inside `destDir`
 *  (collision-resolved). Returns the created archive path. */
export const fsZip = (sources: string[], destDir: string, archiveName: string) =>
  invoke<string>('fs_zip', { sources, destDir, archiveName });
/** Extract a ZIP `archive`, into `destDir` or — when omitted — a new sibling
 *  folder named after the archive. Returns the destination folder path. */
export const fsUnzip = (archive: string, destDir?: string) =>
  invoke<string>('fs_unzip', destDir ? { archive, destDir } : { archive });
/** Set an image file as the desktop wallpaper (Windows / macOS / GNOME). */
export const fsSetWallpaper = (path: string) => invoke<void>('fs_set_wallpaper', { path });
/** Open a path with the OS default app (file) or file manager (dir). */
export const fsOpenDefault = (path: string) => invoke<void>('fs_open_default', { path });
/** Reveal a path in the OS file manager, selecting it. */
export const fsRevealInDir = (path: string) => invoke<void>('fs_reveal_in_dir', { path });
/** Open the OS-native Properties dialog for a path (Windows property sheet /
 *  macOS Finder Get Info / Linux FileManager1 D-Bus). */
export const fsShowProperties = (path: string) => invoke<void>('fs_show_properties', { path });
/** Native system icon for a query (a file extension like ".rs", or an absolute
 *  path — `.exe` yields its embedded icon), as a `data:image/png;base64,…` URI. */
export const fsIcon = (query: string, size: number) => invoke<string>('fs_icon', { query, size });
/** Start watching `path` for changes (replaces any prior watch). Emits the
 *  `arbor://fs-changed` Tauri event on any change in the directory. */
export const fsWatchStart  = (path: string) => invoke<void>('fs_watch_start', { path });
/** Stop the active filesystem watch. */
export const fsWatchStop   = () => invoke<void>('fs_watch_stop');

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
