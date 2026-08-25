/**
 * Bennu project store — the open Java project + its file tree + open-editor model.
 *
 * `openProject` resolves the manifest (modules / JDK / capabilities) and the file
 * tree; file source text is read lazily via `bennu_read_file` and cached (so
 * re-selecting a tab is instant and edits aren't re-read off disk). The editor
 * fan-out owns the live buffer: it pushes edits via `setSource(path, text)` and
 * reads `sourceOf(path)` / `activeSource`.
 *
 * Rune store — the single approved shape: private `$state`, a returned object of
 * getters + methods (see CLAUDE.md · "Store pattern").
 *
 * MOCK fallback — see `bennu-mock.ts`. `openProject` tries real IPC first and, if
 * `bennu-be` isn't attached (calls reject / BackendNotRunning), gracefully falls
 * back to the demo project so the shell is validatable without the backend. There
 * is also an explicit `loadDemo()` affordance. Remove both when the BE is live.
 */

import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import {
  moveToPackage as ipcMoveToPackage,
  openProject as ipcOpenProject,
  activateProject as ipcActivateProject,
  projectTree as ipcProjectTree,
  readFile as ipcReadFile,
  writeFile as ipcWriteFile,
  fileStamps as ipcFileStamps,
  renamePath as ipcRenamePath,
  isExternallyModifiedError,
  projectDiagnostics as ipcProjectDiagnostics,
} from '$lib/ipc/bennu';
// Splicing byte-offset edits into a source string — shared with the rename-preview apply, which is
// where it started.
import { applyByteEdits } from '$lib/components/bennu/rename-apply';
// Which files open as a preview instead of as text — one predicate, so the store (which
// decides whether to read the file at all), `saveText` (which refuses to write one) and the
// editor (which decides what to mount) cannot disagree about what has a buffer behind it.
import { opensAsPreview } from '$lib/utils/preview-files';
// Live re-index — kept in its own IPC file to avoid racing edits on index.ts.
import { didChange as ipcDidChange } from '$lib/ipc/bennu/nav';
// Local history — told about the one kind of change the editor cannot infer from its own
// actions. Kept in its own IPC file, like the rest of the per-domain bennu surface.
import { noteExternal as ipcNoteExternal } from '$lib/ipc/bennu/history';
// The Problems store — a save triggers a silent cross-file re-validation that refreshes it.
import { bennuDiagnosticsStore } from './diagnostics.svelte';
// Workspace store owns the SET of named workspaces + persistence; this store is the live runtime
// of the ACTIVE workspace only. It reports its session snapshot up on every change (see
// `persistWorkspace`). Circular import is safe: neither store touches the other at construction.
import { workspacesStore } from './workspaces.svelte';
// Autosave gate — the user's persisted preference (config-backed).
import { bennuSettingsStore } from './settings.svelte';
import type { ProjectSession } from '$lib/ipc/bennu/config';
import type { ProjectInfo, SourceEdit, TreeNode } from '$lib/types/bennu';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
// MOCK — remove when bennu-be serves real data.
import { DEMO_PROJECT, DEMO_TREE, DEMO_ROOT, isDemoPath, demoReadFile } from './bennu-mock';

/** Extensions Bennu refuses to open in the text editor: opening a large binary would
 *  make `bennu_read_file` (UTF-8 decode) choke — a `.xcf` once froze the window. The
 *  guard is by extension (cheap, no read).
 *
 *  **Previewed files are not in this set** — images and `.docx` open as a preview instead (see
 *  {@link opensAsPreview}). What makes that safe is that they never enter the source cache at all: no
 *  text is read for them, so nothing downstream can mistake one for an empty buffer and write it
 *  back. `saveText` refuses them outright as the second line of that defence. */
const BINARY_EXTENSIONS = new Set([
  'svgz', 'xcf', 'psd', 'ai',
  'pdf', 'zip', 'jar', 'war', 'ear', 'class', 'exe', 'dll', 'so', 'dylib', 'bin',
  'o', 'obj', 'a', 'lib', '7z', 'gz', 'bz2', 'xz', 'tar', 'rar', 'iso', 'dmg',
  'mp3', 'mp4', 'm4a', 'wav', 'flac', 'ogg', 'avi', 'mov', 'mkv', 'webm',
  // `.eot` stays: no engine loads it, so there is nothing a viewer could show. Its three
  // siblings moved to `opensAsPreview` — see `isFontFile`.
  'eot', 'db', 'sqlite', 'mdb', 'keystore', 'jks',
]);

function isBinaryPath(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  if (dot < 0) return false;
  return BINARY_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}

/** Canonical form of a file/dir path used as a tab key + `activeFilePath` + BE argument.
 *  Forward slashes only: Windows accepts them for every FS op, and the BE keys files by
 *  forward-slash paths (JSP include targets, the class index, the form-analysis include
 *  graph), while the OS file picker + project tree hand us native `\`. Normalizing at the
 *  store boundary means a file opened via ANY caller keys the SAME tab (no duplicates) and
 *  the active path matches the BE's string-keyed lookups. Drive-letter case is left as-is
 *  (Windows FS is case-insensitive; the BE compares paths component-wise). */
function canonPath(p: string): string {
  return p.replace(/\\/g, '/');
}

/** A project-tree node with every `path` in the subtree canonicalized (so tree selection —
 *  `selectedId` vs a node's id — matches the canonical `activeFilePath`). */
function canonTree(node: TreeNode): TreeNode {
  return { ...node, path: canonPath(node.path), children: node.children.map(canonTree) };
}

/** Debounce (ms) for the live re-index `bennu_did_change` on editor edits — long
 *  enough that a burst of keystrokes coalesces into one BE patch, short enough that
 *  completion/definition reflect an edit almost immediately. Never blocks typing
 *  (the call is fire-and-forget on the BE blocking pool). */
const REINDEX_DEBOUNCE_MS = 400;

/** Idle delay (ms) before autosave writes a modified buffer. Long enough that a burst of typing
 *  coalesces into one save; short enough that "did it save?" is never a worry. Save-on-tab-switch and
 *  save-on-window-blur cover the cases where you leave before this fires. */
const AUTOSAVE_DEBOUNCE_MS = 1200;

function createProjectStore() {
  let project = $state<ProjectInfo | null>(null);
  let tree = $state<TreeNode | null>(null);
  // True while the open project is the mock demo (backend absent / explicit demo).
  // MOCK — remove when bennu-be serves real data.
  let isDemo = $state(false);
  // Recently-opened project roots (session-only; the switcher lists them).
  let recentProjects = $state<string[]>([]);
  // Source cache keyed by absolute path. SvelteMap so reads are reactive.
  const sources = new SvelteMap<string, string>();
  // Encoding the file was decoded from, keyed by path (Phase 0 keeps it for later
  // round-tripping; the editor just displays the text).
  const encodings = new SvelteMap<string, string>();

  // ── External changes ──────────────────────────────────────────────────────────
  // The on-disk stamp each cached buffer corresponds to (see `FileStamp`). Every save
  // passes it back as the overwrite guard, and `checkExternalChanges` compares against a
  // fresh stat to notice a file something else rewrote. Plain Map — nothing renders a stamp.
  const stamps = new Map<string, string>();
  // Paths whose on-disk content diverged from our buffer AND whose buffer has unsaved
  // edits, so neither side can be discarded without asking. Reactive: the tab strip badges
  // them and the conflict modal is driven off it. Autosave is suspended for these — it
  // would only produce refused writes and a toast per attempt.
  const conflicted = new SvelteSet<string>();
  let activeFilePath = $state<string | null>(null);
  let openFilePaths = $state<string[]>([]);

  // ── Where the caret was, per open tab ─────────────────────────────────────────
  // Persisted with the session, so a restart reopens the tabs *at the line you were on* rather
  // than at the top of each file. Absolute-path keyed and global like `sources`, because a file
  // opened from another workspace project is the same buffer wherever it is listed.
  //
  // Plain Map: nothing renders a caret from here. The editor reads it once, when it mounts a tab
  // it has no live view state for; from then on the live per-tab snapshot in `BennuEditor` is the
  // finer answer (it carries the scroll offset too) and this is only what gets written to disk.
  const carets = new Map<string, { line: number; col: number }>();

  /** `"line:col"` ⇄ the pair, for the persisted (TOML-friendly) form. Tolerant on the way in:
   *  anything that is not two positive integers is "no remembered caret". */
  function parseCaret(text: string): { line: number; col: number } | null {
    const m = /^(\d+):(\d+)$/.exec(text.trim());
    if (!m) return null;
    const line = Number(m[1]);
    const col = Number(m[2]);
    return line > 0 && col > 0 ? { line, col } : null;
  }

  // ── Workspace (N projects) ────────────────────────────────────────────────────
  // The flat `project`/`tree`/`openFilePaths`/`activeFilePath` above are the ACTIVE project's
  // live, reactive view. The other workspace projects live stashed in `sessions` (plain map —
  // inactive projects don't need reactivity); switching is stash-current + load-target, so it's
  // instant with no reopen. `sources`/`encodings` stay global (absolute-path keyed), so a file
  // opened from another project ("foreign") caches the same way and never duplicates.
  interface StashedSession {
    info: ProjectInfo;
    tree: TreeNode | null;
    openFilePaths: string[];
    activeFilePath: string | null;
  }
  const sessions = new Map<string, StashedSession>();
  // The workspace member roots (canonical), in switch order. Always includes the active root.
  let workspaceRoots = $state<string[]>([]);

  // Live re-index — one pending debounce timer per edited path. On each edit we
  // reset the path's timer; when it fires we hand the BE the current full text via
  // `bennu_did_change` so it patches the index. Demo paths never fire (no BE).
  const reindexTimers = new Map<string, ReturnType<typeof setTimeout>>();

  // ── Dirty tracking + autosave ──────────────────────────────────────────────────
  // `savedContent` holds each open file's last-SAVED text; a file is dirty when its live buffer
  // differs. Comparing content (not a "touched" flag) keeps a file from re-marking dirty when its
  // controlled value re-syncs from the cache after a save. `dirty` is reactive so tabs can badge it.
  const savedContent = new Map<string, string>();
  const dirty = new SvelteSet<string>();
  // One pending autosave timer per edited path (mirrors `reindexTimers`).
  const autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function autosaveEnabled(): boolean {
    return bennuSettingsStore.autosave;
  }

  /** Recompute `path`'s dirty state from its live text vs the last-saved snapshot. */
  function markDirty(path: string, text: string) {
    if (text !== (savedContent.get(path) ?? text)) dirty.add(path);
    else dirty.delete(path);
  }

  function cancelAutosave(path: string) {
    const t = autosaveTimers.get(path);
    if (t !== undefined) { clearTimeout(t); autosaveTimers.delete(path); }
  }

  /** Debounced autosave for `path`: after an idle it saves the buffer IF it's still dirty, autosave
   *  is on, and it's a real (non-demo) file. Reset on every edit, like the re-index debounce.
   *
   *  A **conflicted** path is skipped: the file changed on disk under an edited buffer, so a
   *  write would be refused by the backend guard anyway — retrying it every 1.2s would just
   *  spin and toast. Autosave resumes for that path once the conflict is resolved. */
  function scheduleAutosave(path: string) {
    cancelAutosave(path);
    if (!autosaveEnabled() || isDemoPath(path) || conflicted.has(path)) return;
    autosaveTimers.set(path, setTimeout(() => {
      autosaveTimers.delete(path);
      if (dirty.has(path)) void saveText(path, sources.get(path) ?? '');
    }, AUTOSAVE_DEBOUNCE_MS));
  }

  /** Save every file with unsaved edits (used by save-on-window-blur). Ungated — the caller decides
   *  when; it saves whatever is dirty, sequentially so the writes don't stampede the BE. A
   *  conflicted path is skipped rather than refused-and-toasted once per blur. */
  async function saveAllDirty() {
    for (const p of [...dirty]) {
      if (conflicted.has(p)) continue;
      await saveText(p, sources.get(p) ?? '');
    }
  }

  /**
   * Notice files that changed on disk behind the editor, and act per file:
   *
   * * buffer **clean** → adopt the new content silently. There is nothing to lose and
   *   nothing to decide; this is what makes a `git checkout` or a generator run just show
   *   up, the way IntelliJ does it.
   * * buffer **dirty** → mark it {@link conflicted}. Both versions matter, so the UI asks.
   *   Autosave stands down for that file until it is resolved.
   *
   * A file that has been **deleted** is neither: it raises no conflict, and its stamp is
   * simply dropped. There is no "version on disk" to weigh the buffer against, so there is
   * nothing to decide — dropping the stamp also lifts the write guard, so a later save
   * recreates the file (the same call the backend allows for exactly this reason). The tab
   * keeps showing what it had; closing it is the user's call, and it is the only gesture that
   * actually agrees with the deletion.
   *
   * Cheap enough to call often (one `stat` per open tab, no reads): the window calls it on
   * focus, on tab activation and on a slow tick while focused. Silent on failure — a poll
   * that can't reach the backend must never interrupt anything.
   */
  async function checkExternalChanges(): Promise<void> {
    if (isDemo) return;
    const watched = openFilePaths.filter((p) => !isDemoPath(p) && stamps.has(p));
    if (!watched.length) return;

    let current;
    try {
      current = await ipcFileStamps(watched);
    } catch {
      return; // BE absent / busy — try again on the next tick
    }

    // Every file whose on-disk state no longer matches what we read. Collected and sent in
    // one call rather than one per file: the poll runs on every focus, and a round trip per
    // watched tab would make noticing a change cost more than the change.
    const changed: string[] = [];

    for (const entry of current) {
      const known = stamps.get(entry.file);
      // `known` may have gone (tab closed, project switched) while the stat was in flight.
      if (known === undefined || known === entry.stamp) continue;
      changed.push(entry.file);

      if (!entry.exists) {
        // Deleted — see the note above. Dropping the stamp both stops the every-tick report
        // and lifts the guard, so the buffer can be written back if the user saves.
        stamps.delete(entry.file);
        continue;
      }

      if (dirty.has(entry.file)) {
        conflicted.add(entry.file);
        cancelAutosave(entry.file);
      } else {
        // eslint-disable-next-line no-await-in-loop
        await adoptFromDisk(entry.file, true);
      }
    }

    // Tell the local history what the outside world did — including the deletions, which is
    // how an `rm` from a terminal ends up in the Deleted list with its content still there.
    // Fire-and-forget and last: it must never delay adopting a change, and a history that
    // cannot be written is not a reason to stop editing.
    if (changed.length && project?.root) {
      void ipcNoteExternal(project.root, changed).catch(() => { /* history is best-effort */ });
    }
  }

  function rememberRecent(root: string) {
    recentProjects = [root, ...recentProjects.filter((p) => p !== root)].slice(0, 10);
  }

  /** Fire `bennu_did_change` for `path` with its current cached text, patching the
   *  BE index. Fire-and-forget: swallow errors (BE absent / not indexed yet) so a
   *  re-index never surfaces as a failure while typing. Skips demo paths. */
  function reindexNow(path: string) {
    const t = reindexTimers.get(path);
    if (t !== undefined) { clearTimeout(t); reindexTimers.delete(path); }
    // MOCK — demo files have no backing project; never call the BE.
    if (isDemoPath(path)) return;
    const text = sources.get(path);
    if (text === undefined) return;
    void ipcDidChange(path, text).catch(() => { /* BE absent / unowned — ignore */ });
  }

  /** Debounced re-index: reset `path`'s timer so a keystroke burst coalesces into
   *  one BE patch. Never blocks the edit that triggered it. */
  function scheduleReindex(path: string) {
    if (isDemoPath(path)) return; // MOCK — no BE for demo files
    const existing = reindexTimers.get(path);
    if (existing !== undefined) clearTimeout(existing);
    reindexTimers.set(path, setTimeout(() => reindexNow(path), REINDEX_DEBOUNCE_MS));
  }

  // ── Live Problems panel: a SILENT cross-file re-validation after a save ───────────────────────
  // Saving a file can change diagnostics in OTHER files (a fixed/added method resolves elsewhere).
  // A save schedules a debounced, silent whole-project re-validation (cheap — the incremental cache
  // makes it near-instant) whose grouped diagnostics refresh the Problems panel, so cross-file
  // effects show without the user re-running "Validate". The debounce also lets the save's live
  // index patch land first, so dependents resolve against the new members.
  let problemsRefreshTimer: ReturnType<typeof setTimeout> | undefined;
  let problemsRefreshToken = 0;
  const PROBLEMS_REFRESH_DEBOUNCE_MS = 600;
  function scheduleProblemsRefresh() {
    const root = project?.root;
    if (!root || isDemo) return;
    // Only once the user has opted into the project-wide view (ran "Validate" at least once) — a
    // save shouldn't flood the panel unasked on a project with thousands of dependency problems.
    if (!bennuDiagnosticsStore.armed) return;
    if (problemsRefreshTimer !== undefined) clearTimeout(problemsRefreshTimer);
    problemsRefreshTimer = setTimeout(() => {
      if (project?.root !== root) return; // switched project during the debounce
      const mine = ++problemsRefreshToken;
      void ipcProjectDiagnostics(root)
        .then((list) => {
          // Ignore a superseded response or one for a project no longer active. `null` = index not
          // ready → leave the panel untouched.
          if (mine !== problemsRefreshToken || project?.root !== root) return;
          if (list) bennuDiagnosticsStore.refreshProjectDiagnostics(list);
        })
        .catch(() => { /* BE absent — leave the panel as-is */ });
    }, PROBLEMS_REFRESH_DEBOUNCE_MS);
  }

  /** Build the active workspace's session snapshot from the live flat state (active project) +
   *  the stashed sessions (the other members). Pure — used both to persist and to flush the live
   *  state into the workspace store before a workspace switch. Empty for the demo / no project. */
  function snapshotSession(): { active_project: string; projects: ProjectSession[] } {
    const activeRoot = project?.root;
    if (!activeRoot || isDemo) return { active_project: '', projects: [] };
    // The ACTIVE root comes from the live flat state, the rest from their stashed sessions.
    // `workspaceRoots` always includes the active root.
    const roots = workspaceRoots.includes(activeRoot) ? workspaceRoots : [activeRoot];
    // The carets ride alongside the tab list, one slot per tab — see `ProjectSession.open_carets`
    // for why it is a parallel array and not a table.
    const caretsFor = (paths: string[]) =>
      paths.map((p) => {
        const c = carets.get(p);
        return c ? `${c.line}:${c.col}` : '';
      });
    const projects = roots.map((r) => {
      if (r === activeRoot) {
        return {
          root: r,
          open_files: openFilePaths,
          active_file: activeFilePath ?? '',
          open_carets: caretsFor(openFilePaths),
        };
      }
      const s = sessions.get(r);
      const paths = s?.openFilePaths ?? [];
      return {
        root: r,
        open_files: paths,
        active_file: s?.activeFilePath ?? '',
        open_carets: caretsFor(paths),
      };
    });
    return { active_project: activeRoot, projects };
  }

  // Persist the active workspace's session (open tabs + active tab per project) debounced, so the
  // next launch restores it. Routes THROUGH the workspace store (which owns the named-workspace
  // list + the `workspace.toml` write). Never persists the demo or a null project; a burst of tab
  // opens/closes coalesces into one write.
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  /** `delay` is longer for the caret, which changes on every arrow key: a tab open is one event
   *  the user is waiting on, a caret move is hundreds a minute, and both write the same file. */
  function persistWorkspace(delay = 300) {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      if (!project || isDemo) return;
      workspacesStore.saveActiveSession(snapshotSession());
    }, delay);
  }
  /** How long the caret must sit still before its position is worth a write. */
  const CARET_PERSIST_MS = 1500;

  /** Open a file as the active tab (loads source + encoding if needed), refusing binaries.
   *  The persistence-free core, shared by the public {@link openFile} (which persists after)
   *  and boot-time restore (which persists once at the end). */
  async function openFileInternal(path: string) {
    path = canonPath(path);
    if (isBinaryPath(path)) {
      toastStore.show(`Can't open ${path.split(/[\\/]/).pop()} — binary file`, 'info');
      return;
    }
    // A previewed file gets a tab but no buffer: the viewer reads the bytes itself, so nothing
    // decodes megabytes of PNG — or a ZIP of XML — as UTF-8 on the way to a text editor.
    if (opensAsPreview(path)) {
      activeFilePath = path;
      if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
      return;
    }
    await ensureLoaded(path);
    activeFilePath = path;
    if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
  }

  /** Read a file's source + encoding into the cache if not already present. The
   *  project root is passed so the backend resolves the pom-declared encoding.
   *  Demo paths are served from the mock (no IPC). */
  async function ensureLoaded(path: string) {
    if (sources.has(path)) return;
    // MOCK — demo files never touch the backend.
    if (isDemoPath(path)) {
      const res = demoReadFile(path);
      sources.set(path, res.text);
      savedContent.set(path, res.text); // freshly loaded → not dirty
      encodings.set(path, res.encoding);
      return;
    }
    try {
      const res = await ipcReadFile(project?.root ?? path, path);
      sources.set(path, res.text);
      savedContent.set(path, res.text);
      encodings.set(path, res.encoding);
      stamps.set(path, res.stamp);
    } catch {
      sources.set(path, '');
      savedContent.set(path, '');
      encodings.set(path, 'utf-8');
      // No stamp: the read failed, so we know nothing about what's on disk. An absent stamp
      // disables the overwrite guard, which is right — refusing a save here would only trap
      // the buffer, and there is no external edit to protect.
      stamps.delete(path);
    }
  }

  function applyProject(info: ProjectInfo, nextTree: TreeNode | null, demo: boolean) {
    project = { ...info, root: canonPath(info.root) };
    tree = nextTree ? canonTree(nextTree) : null;
    isDemo = demo;
    rememberRecent(info.root);
    // Reset the open-file model for the new project.
    activeFilePath = null;
    openFilePaths = [];
    sources.clear();
    encodings.clear();
    carets.clear();
    // Drop dirty/save state from the previous project.
    savedContent.clear();
    dirty.clear();
    // …and its external-change bookkeeping: a stamp belongs to a buffer we no longer hold.
    stamps.clear();
    conflicted.clear();
    // Drop any pending re-index / autosave timers from the previous project.
    for (const t of reindexTimers.values()) clearTimeout(t);
    reindexTimers.clear();
    for (const t of autosaveTimers.values()) clearTimeout(t);
    autosaveTimers.clear();
  }

  /** MOCK — load the built-in demo project (explicit affordance + BE-down fallback). */
  function loadDemoProject() {
    applyProject(DEMO_PROJECT, DEMO_TREE, true);
  }

  /** Stash the active project's live state into `sessions` (so a later switch restores it). */
  function stashActive() {
    if (project) {
      sessions.set(project.root, { info: project, tree, openFilePaths, activeFilePath });
    }
  }

  /** Load a stashed session into the flat (active) state. Sources stay in the global cache, so
   *  the active file's text is fetched lazily by the caller if not already present. */
  function loadSession(s: StashedSession) {
    project = s.info;
    tree = s.tree;
    openFilePaths = s.openFilePaths;
    activeFilePath = s.activeFilePath;
    isDemo = false;
  }

  /** Fetch `root`'s file tree in the background, updating BOTH the stashed session and — when it's
   *  the active project — the live `tree`. The root guard drops a stale tree from a superseded open. */
  function loadTreeInto(root: string) {
    void ipcProjectTree(root)
      .then((t) => {
        const ct = canonTree(t);
        const s = sessions.get(root);
        if (s) s.tree = ct;
        if (project?.root === root) tree = ct;
      })
      .catch(() => { /* leave the tree empty — the project still opened */ });
  }

  /** Ensure `path`'s source is loaded, then return it (''), for consumers (rename
   *  apply) that need a file's current text whether or not a tab is open. */
  async function loadText(path: string): Promise<string> {
    await ensureLoaded(path);
    return sources.get(path) ?? '';
  }

  /**
   * Write `text` to `path`: update the cache, persist to disk (`bennu_write_file`, skipped
   * for demo/BE-absent), and flush a live re-index. The editor's controlled `value`
   * re-syncs from the updated cache, so an open buffer reflects the change.
   *
   * ## The overwrite guard
   *
   * The buffer's stamp rides along, so the backend **refuses** the write when something
   * else changed the file since we read it. That refusal is the point: autosave used to
   * overwrite an external edit silently, which is data loss on a timer. On a refusal the
   * path is marked {@link conflicted} — the buffer keeps its edits, stays dirty, autosave
   * stands down, and the UI asks which side wins.
   *
   * `force` skips the guard, for the "keep mine" resolution. It is the only way to overwrite
   * a file that changed, and it is never automatic.
   *
   * Returns whether the content reached disk. Callers that chain filesystem work on a save
   * (the package move) must not proceed on `false`.
   */
  async function saveText(path: string, text: string, force = false): Promise<boolean> {
    // A previewed file has no buffer, so anything asking to write one is asking to overwrite the
    // file with an empty string. Refused here rather than trusted not to happen: a stray Ctrl+S on
    // a preview tab would otherwise be silent data loss.
    if (opensAsPreview(path)) return false;
    sources.set(path, text);
    if (isDemoPath(path)) {
      // MOCK — no disk behind a demo file; treat it as saved so the tab goes clean.
      markSaved(path, text);
      return true;
    }
    try {
      const res = await ipcWriteFile(
        project?.root ?? path,
        path,
        text,
        force ? undefined : stamps.get(path),
      );
      encodings.set(path, res.encoding);
      stamps.set(path, res.stamp);
      conflicted.delete(path);
    } catch (err) {
      if (isExternallyModifiedError(err)) {
        // Somebody else owns the newer version. Keep our edits, keep the file dirty, stop
        // autosave from retrying, and surface it — `dirty` must NOT be cleared, or the
        // unsaved work would look saved.
        conflicted.add(path);
        cancelAutosave(path);
        toastStore.show(
          `${path.split(/[\\/]/).pop()} changed on disk — your edits were not overwritten`,
          'warning',
        );
        return false;
      }
      /* BE absent / other I/O failure — cache updated, disk not (best-effort, as before) */
    }
    // Now clean: record the saved snapshot, clear the dirty mark, and drop a pending autosave (this
    // write supersedes it). Best-effort: even if the disk write failed above, we don't loop-retry.
    markSaved(path, text);
    reindexNow(path);
    // Refresh the Problems panel project-wide (silent, debounced) so cross-file effects of this
    // save appear without a manual "Validate".
    scheduleProblemsRefresh();
    return true;
  }

  /**
   * Apply byte-offset edits across files: group by file, splice each one, persist.
   *
   * Here rather than in each caller because there are three of them — a language server's
   * `workspace/applyEdit`, a code action's edit list, and the edits a file rename implies — and every
   * one needs the same steps: read the file's *current* text (which is the buffer when a tab has it),
   * splice byte spans, write it back through the same guard an ordinary save goes through.
   *
   * Returns how many files could not be written. Counted per file rather than aborting: the others
   * are still correct, and stopping halfway leaves a project that neither builds nor explains itself.
   */
  async function applyEditsAcrossFiles(edits: readonly SourceEdit[]): Promise<number> {
    const byFile = new Map<string, SourceEdit[]>();
    for (const e of edits) {
      const list = byFile.get(e.file);
      if (list) list.push(e);
      else byFile.set(e.file, [e]);
    }
    let failed = 0;
    for (const [file, fileEdits] of byFile) {
      try {
        const current = await loadText(file);
        if (!(await saveText(file, applyByteEdits(current, fileEdits)))) failed += 1;
      } catch {
        failed += 1;
      }
    }
    return failed;
  }

  /** Record `text` as `path`'s on-disk baseline: clean, no pending autosave, not conflicted. */
  function markSaved(path: string, text: string) {
    savedContent.set(path, text);
    dirty.delete(path);
    conflicted.delete(path);
    cancelAutosave(path);
  }

  /**
   * Adopt the on-disk content of `path`, discarding whatever the buffer held.
   *
   * Serves both the explicit "take theirs" resolution and the silent refresh of a clean
   * buffer — and those differ in one way that matters. `onlyIfClean` re-checks dirtiness
   * **after** the read: the caller decided the buffer was clean before awaiting, and a
   * keystroke landing during that await would otherwise be thrown away by the very refresh
   * that was supposed to be lossless. The explicit resolution passes `false`, because there
   * discarding the buffer is exactly what was asked for.
   */
  async function adoptFromDisk(path: string, onlyIfClean = false) {
    try {
      const res = await ipcReadFile(project?.root ?? path, path);
      if (onlyIfClean && dirty.has(path)) {
        // Typed while we were reading. Don't touch the buffer; record the stamp so the next
        // poll sees the file has moved on and raises this as a proper conflict instead.
        stamps.set(path, res.stamp);
        conflicted.add(path);
        cancelAutosave(path);
        return;
      }
      sources.set(path, res.text);
      encodings.set(path, res.encoding);
      stamps.set(path, res.stamp);
      markSaved(path, res.text);
      reindexNow(path);
    } catch {
      /* unreadable (deleted mid-flight) — leave the buffer alone; the conflict flag stands */
    }
  }

  return {
    get project()        { return project; },
    get tree()           { return tree; },
    get capabilities()   { return project?.capabilities ?? null; },
    /** Which manifest governs the active project. `'maven'` with nothing open, so the
     *  Java-only chrome keeps its usual (disabled) shape rather than collapsing. */
    get kind()           { return project?.kind ?? 'maven'; },
    /** True when the active project is a Cargo one — gate Java-only UI (JDK, Maven,
     *  Dependencies, Generate, validation, index status) on `!isCargo`. */
    get isCargo()        { return project?.kind === 'cargo'; },
    get activeFilePath() { return activeFilePath; },
    /**
     * `path` written relative to the open project's root, forward slashes — the root itself
     * being `.`. Falls back to the absolute path when it is outside the project, which is the
     * honest answer rather than a chain of `../`.
     *
     * On the store because the root is: every panel that shows a path to a person wants it
     * short, and a second copy of this arithmetic is a second place to get the trailing
     * separator wrong.
     */
    relativePath(path: string): string {
      const fwd = canonPath(path);
      const root = project?.root;
      if (!root) return fwd;
      const rootFwd = canonPath(root).replace(/\/+$/, '');
      if (fwd === rootFwd) return '.';
      const prefix = rootFwd + '/';
      return fwd.startsWith(prefix) ? fwd.slice(prefix.length) : fwd;
    },
    get openFilePaths()  { return openFilePaths; },

    /**
     * Remember where the caret is in `path`, so a restart reopens the tab on that line.
     *
     * Called on every caret move, which is why the write is debounced hard (see
     * {@link CARET_PERSIST_MS}) and why the map is not reactive: nothing on screen reads it —
     * the editor keeps its own, finer, per-tab view state for the length of a session.
     */
    rememberCaret(path: string, line: number, col: number) {
      const key = canonPath(path);
      const before = carets.get(key);
      if (before && before.line === line && before.col === col) return;
      carets.set(key, { line, col });
      persistWorkspace(CARET_PERSIST_MS);
    },

    /** The remembered caret for `path`, or `null`. The editor asks once, when it opens a tab it
     *  has no live view state for — a restored session being exactly that case. */
    caretOf(path: string): { line: number; col: number } | null {
      return carets.get(canonPath(path)) ?? null;
    },

    /** True while the open project is the mock demo. MOCK — remove with the mock. */
    get isDemo()         { return isDemo; },
    /** Recently-opened project roots (most recent first). */
    get recentProjects() { return recentProjects; },
    /** The workspace member projects for the switcher: `{ root, name }` in switch order. */
    get workspaceProjects(): { root: string; name: string }[] {
      return workspaceRoots.map((r) => {
        const info = r === project?.root ? project : sessions.get(r)?.info;
        return { root: r, name: info?.name ?? (r.split('/').pop() || r) };
      });
    },
    /** True when the workspace holds more than one project (drives showing the switcher UI). */
    get hasWorkspace(): boolean { return workspaceRoots.length > 1; },
    /** Every open project root. Read by the filesystem watcher, which wants the whole set rather
     *  than one root at a time — see `bennu_watch_roots`. */
    get workspaceRoots(): string[] { return workspaceRoots; },
    /** True when `path` isn't under the active project's root — a "foreign" tab (opened from
     *  another workspace project), which the tab strip badges (mirrors Corvus). */
    isForeign(path: string): boolean {
      const root = project?.root;
      if (!root) return false;
      const p = canonPath(path).toLowerCase();
      const r = root.toLowerCase();
      return !(p === r || p.startsWith(r.endsWith('/') ? r : `${r}/`));
    },
    /** Encoding of the active file (defaults to `UTF-8`). */
    get activeEncoding() { return activeFilePath ? (encodings.get(activeFilePath) ?? 'UTF-8') : null; },
    /** Cached source of the active file (or '' when none / not yet loaded). */
    get activeSource()   { return activeFilePath ? (sources.get(activeFilePath) ?? '') : ''; },

    /** Cached source for `path` ('' when not loaded). */
    sourceOf(path: string): string { return sources.get(path) ?? ''; },
    /** Decoded encoding for `path` (defaults to `UTF-8`). */
    encodingOf(path: string): string { return encodings.get(path) ?? 'UTF-8'; },

    /** Force-reload `path` from disk, discarding its cached source/encoding — e.g. after a
     *  decompiled tab's real sources were downloaded and the backend rewrote the file. A no-op for a
     *  path that was never loaded (nothing open to refresh). Updating the reactive `sources` map
     *  re-renders the open editor. */
    async reload(path: string): Promise<void> {
      const p = canonPath(path);
      if (!sources.has(p)) return; // nothing open to refresh
      try {
        // Read-then-set (no intermediate clear) so the open editor never flashes empty.
        const res = await ipcReadFile(project?.root ?? p, p);
        sources.set(p, res.text);
        savedContent.set(p, res.text);
        encodings.set(p, res.encoding);
        // The buffer now matches disk, so the new stamp is the baseline and any conflict
        // over this file is settled by definition.
        stamps.set(p, res.stamp);
        dirty.delete(p);
        conflicted.delete(p);
      } catch { /* keep the current content on a read failure */ }
    },

    /** Open a project as a **new single-project workspace** (replaces the current one, dropping
     *  the other members). Use {@link addProject} to add to the existing workspace instead.
     *  Tries real IPC; on backend-absent failure, falls back to the demo project so the shell
     *  stays populated. MOCK — drop the catch when bennu-be is live. */
    async openProject(dir: string) {
      try {
        const info = await ipcOpenProject(dir);
        // Show the project shell immediately, then load the (potentially large) tree in the
        // background — awaiting it here is what made opening lag.
        applyProject(info, null, false); // resets the flat state + source cache
        sessions.clear();                // a fresh workspace: drop the previous members
        workspaceRoots = [project!.root];
        loadTreeInto(project!.root);
        persistWorkspace();
      } catch (err) {
        // MOCK — bennu-be not attached: fall back to the demo so opening the
        // window still shows a populated tree. Remove when the BE serves data.
        loadDemoProject();
        throw err; // let callers still observe the failure (they ignore it)
      }
    },

    /** Add a project to the current workspace and switch to it (keeping the existing members).
     *  The current project's tabs are stashed; the new project opens with an empty tab set. */
    async addProject(dir: string) {
      let info;
      try {
        info = await ipcOpenProject(dir);
      } catch {
        return; // BE absent / not a project — no-op (openProject owns the demo fallback)
      }
      const root = canonPath(info.root);
      // Already a member → just switch to it.
      if (workspaceRoots.includes(root)) { void this.switchProject(root); return; }
      stashActive();
      const canonInfo: ProjectInfo = { ...info, root };
      sessions.set(root, { info: canonInfo, tree: null, openFilePaths: [], activeFilePath: null });
      loadSession(sessions.get(root)!);
      workspaceRoots = [...workspaceRoots, root];
      rememberRecent(root);
      loadTreeInto(root);
      persistWorkspace();
    },

    /** Switch the active project to `root` (an existing workspace member). Instant — the target's
     *  state is already in memory; only the active file's source is (lazily) ensured. No-op when
     *  `root` is already active or isn't a member. */
    async switchProject(root: string) {
      root = canonPath(root);
      if (project?.root === root) return;
      const s = sessions.get(root);
      if (!s) return;
      stashActive();
      loadSession(s);
      // Somebody is now looking at this one. Idempotent when its server is already up — and it
      // is what starts one for a member that was restored in the background.
      void ipcActivateProject(root).catch(() => {});
      if (activeFilePath) await ensureLoaded(activeFilePath);
      persistWorkspace();
    },

    /** Remove `root` from the workspace (dropping its stashed session). If it was active, switch
     *  to another member (or clear when it was the last). */
    closeProject(root: string) {
      root = canonPath(root);
      if (!workspaceRoots.includes(root)) return;
      const wasActive = project?.root === root;
      sessions.delete(root);
      workspaceRoots = workspaceRoots.filter((r) => r !== root);
      if (wasActive) {
        const next = workspaceRoots[0];
        if (next && sessions.has(next)) {
          loadSession(sessions.get(next)!);
          if (activeFilePath) void ensureLoaded(activeFilePath);
        } else {
          project = null; tree = null; openFilePaths = []; activeFilePath = null;
        }
      }
      persistWorkspace();
    },

    /** MOCK — explicitly load the built-in demo project. Remove with the mock. */
    loadDemo() { loadDemoProject(); },

    /** Re-fetch the active project's file tree (e.g. after creating a new file on disk, so it
     *  appears in the Project tool). No-op when no project is open / on the demo. */
    refreshTree() {
      const r = project?.root;
      if (r && !isDemo) loadTreeInto(r);
    },

    /**
     * Re-fetch the tree of `root`, whichever project it belongs to.
     *
     * What the filesystem watcher calls. Keyed by root and not "the active one" because a change
     * can land in a workspace member that is not on screen: reloading the active tree for it would
     * be a reload that fixes nothing, and the member's tree would stay wrong until it was next
     * switched to. A root this window does not have open is ignored.
     */
    refreshTreeOf(root: string) {
      if (isDemo) return;
      if (project?.root === root) {
        loadTreeInto(root);
        return;
      }
      if (workspaceRoots.includes(root)) loadTreeInto(root);
    },

    /** The active workspace's session snapshot (open tabs per member project). The workspace store
     *  calls this to flush the live state into a workspace before switching to another. */
    snapshotSession,

    /** Clear the active view entirely (no project / tabs) — used when the workspace store activates
     *  a freshly-created empty workspace. Persistence-free; the store owns the write. */
    clearAll() {
      sessions.clear();
      workspaceRoots = [];
      project = null;
      tree = null;
      openFilePaths = [];
      activeFilePath = null;
      isDemo = false;
      stamps.clear();
      conflicted.clear();
      for (const t of reindexTimers.values()) clearTimeout(t);
      reindexTimers.clear();
    },

    /** Load a whole workspace's projects into the live runtime — the boot-restore / workspace-switch
     *  entry point (driven by the workspace store, which owns the persisted set). Opens each
     *  project's manifest, stashes a session per project, then activates `active` (or the first that
     *  opened) and loads its active file. A vanished project is skipped; nothing opened leaves the
     *  window empty (no demo fallback, no error). Persistence-free — the store persists after. */
    async loadWorkspace(projects: ProjectSession[], active: string) {
      sessions.clear();
      workspaceRoots = [];
      project = null;
      tree = null;
      openFilePaths = [];
      activeFilePath = null;
      isDemo = false;
      // Which one will end up on screen, decided before the loop so the others can be opened
      // WITHOUT a language server. Opening five Cargo projects used to warm-start five
      // rust-analyzers — one indexing run and up to a gigabyte apiece — for four projects
      // nobody was looking at. The rest start on the first request against one of their files,
      // or when `switchProject` announces them.
      const wanted = canonPath(active);
      for (const p of projects) {
        let info;
        // eslint-disable-next-line no-await-in-loop
        try {
          info = await ipcOpenProject(p.root, canonPath(p.root) === wanted);
        } catch { continue; } // a project that's gone
        const root = canonPath(info.root);
        const paths = p.open_files.map(canonPath);
        // The carets ride positionally alongside the tabs, and a session written before they
        // existed simply has none — hence the index-wise read rather than a zip.
        paths.forEach((path, i) => {
          const caret = parseCaret(p.open_carets?.[i] ?? '');
          if (caret) carets.set(path, caret);
        });
        sessions.set(root, {
          info: { ...info, root },
          tree: null,
          openFilePaths: paths,
          activeFilePath: p.active_file ? canonPath(p.active_file) : null,
        });
        workspaceRoots = [...workspaceRoots, root];
        rememberRecent(root);
        loadTreeInto(root);
      }
      if (!workspaceRoots.length) return; // nothing opened (all gone / BE down)
      // Activate the remembered active project (or the first that opened), then load its active file.
      const target = workspaceRoots.includes(wanted) ? wanted : workspaceRoots[0];
      // The fallback case is the one that needs saying so: the remembered project was gone, so
      // the project now on screen is one that was opened as a background member and has no
      // server. Announcing it is what starts one.
      if (target !== wanted) void ipcActivateProject(target).catch(() => {});
      loadSession(sessions.get(target)!);
      if (!activeFilePath && openFilePaths.length) activeFilePath = openFilePaths[0];
      if (activeFilePath) await ensureLoaded(activeFilePath);
    },

    /** Open a file as the active editor tab (loads its source + encoding if needed).
     *  Binary files are refused with a toast (opening one would choke the UTF-8 read
     *  — a `.xcf` once froze the window). */
    async openFile(path: string) {
      // Canonicalize (forward slashes) so a file opened via different callers — the project
      // tree (native `\`), a JSP include go-to or the class index (BE forward-slash) — keys
      // the SAME tab instead of a duplicate, and the active path matches the BE's
      // forward-slash file keys (e.g. the form-analysis include graph). The open logic lives
      // in `openFileInternal` (shared with boot restore); this wrapper persists the session.
      await openFileInternal(path);
      persistWorkspace();
    },

    /** Apply byte-offset edits across files (a server's `applyEdit`, a code action, a rename's
     *  implied edits). Returns how many files could not be written. */
    applyEdits: applyEditsAcrossFiles,

    /**
     * Rename the file at `path` to `newName` in the same directory.
     *
     * The order matters and is the backend's: it asks the language server what the rename implies
     * *before* moving anything, then moves, then hands back the edits — so a failed move leaves both
     * the tree and the code untouched. Applying those edits is this side's job, because they go
     * through the same write path as a save (and so through the same external-change guard).
     *
     * The buffer is saved first, for the same reason `moveFileToPackage` does it: the rename carries
     * whatever is on disk, and unsaved edits left behind would be edits to a path that no longer
     * exists.
     *
     * Returns the new path. Throws with a message the caller can show — the refusals (a name already
     * taken, a missing directory) are the interesting outcomes, not exceptional ones.
     */
    async renameFile(path: string, newName: string): Promise<string> {
      const trimmed = newName.trim();
      if (!trimmed) throw new Error('A file needs a name');
      const parent = path.replace(/[\\/][^\\/]*$/, '');
      const target = canonPath(`${parent}/${trimmed}`);
      if (target === canonPath(path)) return path;

      const wasOpen = openFilePaths.includes(path);
      const source = sources.get(path) ?? (await loadText(path));
      if (dirty.has(path) && !(await saveText(path, source))) {
        throw new Error('The file changed on disk — resolve that first, then rename it');
      }

      const res = await ipcRenamePath(project?.root ?? '', path, target);
      const newPath = canonPath(res.new_path);

      // Carry the cached text / encoding to the new key so a reopened tab is instant, and drop the
      // old stamp: it describes a path that no longer exists, and a stale one would make the next
      // save refuse for no reason.
      sources.set(newPath, source);
      const enc = encodings.get(path);
      if (enc) encodings.set(newPath, enc);
      savedContent.set(newPath, source);
      sources.delete(path);
      savedContent.delete(path);
      stamps.delete(path);
      dirty.delete(path);
      conflicted.delete(path);
      try {
        const [fresh] = await ipcFileStamps([newPath]);
        if (fresh?.stamp) stamps.set(newPath, fresh.stamp);
      } catch { /* the guard simply stays off for this path until it is re-read */ }

      // Re-point the tab only if one was open: renaming from the tree must not open the file.
      openFilePaths = openFilePaths.filter((p) => p !== path);
      if (wasOpen) await openFileInternal(newPath);
      else if (activeFilePath === path) activeFilePath = openFilePaths[0] ?? null;

      // AFTER the move: the edits are expressed against files as they are now, and one of them is
      // very often the renamed file itself.
      if (res.edits.length) {
        const failed = await applyEditsAcrossFiles(res.edits);
        if (failed) {
          throw new Error(`Renamed, but ${failed} file(s) referring to it could not be updated`);
        }
      }
      if (project?.root && !isDemo) loadTreeInto(project.root);
      persistWorkspace();
      return newPath;
    },

    /** Move the file at `path` into the folder matching the `package` it declares (the filesystem
     *  alternative to the change-package edit). Saves the buffer first, moves it on disk, then
     *  re-points the tab + refreshes the tree to the new location. Returns the new path, or throws
     *  with a message the caller can surface. */
    async moveFileToPackage(path: string): Promise<string> {
      const source = sources.get(path) ?? (await loadText(path));
      // Persist the buffer so the on-disk move carries the current text. A refused save means
      // the file changed under us — moving it would carry the stale text to the new path and
      // lose the other edit, so stop and let the conflict be resolved first.
      if (!(await saveText(path, source))) {
        throw new Error('The file changed on disk — resolve that first, then move it');
      }
      const res = await ipcMoveToPackage(path, source);
      const newPath = canonPath(res.new_path);
      // Carry the cached source/encoding to the new key so the reopened tab is instant.
      sources.set(newPath, source);
      const enc = encodings.get(path);
      if (enc) encodings.set(newPath, enc);
      savedContent.set(newPath, source);
      // The rename happened behind us, so the new path's stamp is unknown — and a stale one
      // would make the next save refuse for no reason. Stat it; if that fails, leave it absent
      // (no stamp = no guard, which is the safe direction).
      stamps.delete(path);
      conflicted.delete(path);
      try {
        const [fresh] = await ipcFileStamps([newPath]);
        if (fresh?.stamp) stamps.set(newPath, fresh.stamp);
      } catch { /* the guard simply stays off for this path until it is re-read */ }
      // Re-point the tab: drop the old path, open the new one, refresh the tree to show the move.
      openFilePaths = openFilePaths.filter((p) => p !== path);
      await openFileInternal(newPath);
      if (project?.root && !isDemo) loadTreeInto(project.root);
      persistWorkspace();
      return newPath;
    },

    /** Close a tab; pick a neighbour as active. */
    closeFile(path: string) {
      const idx = openFilePaths.indexOf(path);
      if (idx === -1) return;
      openFilePaths = openFilePaths.filter((p) => p !== path);
      // The conflict flag deliberately SURVIVES the close. Closing a tab doesn't discard its
      // unsaved edits here (`dirty` and the buffer both persist, so reopening restores them),
      // so clearing the flag would re-arm autosave on a file that is still mid-conflict — the
      // exact overwrite this guards against. `conflictedPaths` hides it from the modal while
      // no tab shows it; the decision comes back with the tab.
      if (activeFilePath === path) {
        activeFilePath = openFilePaths.length
          ? openFilePaths[Math.min(idx, openFilePaths.length - 1)]
          : null;
        if (activeFilePath) void ensureLoaded(activeFilePath); // a restored neighbour may be lazy
      }
      persistWorkspace();
    },

    /** Close every tab except `path` — it becomes the active one (so the surviving
     *  tab is always focused, regardless of what was active before). No-op when the
     *  path isn't open. */
    closeOthers(path: string) {
      if (!openFilePaths.includes(path)) return;
      openFilePaths = [path];
      activeFilePath = path;
      void ensureLoaded(path);
      persistWorkspace();
    },

    /** Close all open tabs. */
    closeAll() {
      openFilePaths = [];
      activeFilePath = null;
      persistWorkspace();
    },

    /** Close every tab to the right of `path` (keeps `path` and everything before it).
     *  If the active tab was among those closed, `path` becomes active so focus stays
     *  on a surviving tab. No-op when the path isn't open. */
    closeToRight(path: string) {
      const idx = openFilePaths.indexOf(path);
      if (idx === -1) return;
      const kept = openFilePaths.slice(0, idx + 1);
      if (kept.length === openFilePaths.length) return; // nothing to the right
      openFilePaths = kept;
      if (activeFilePath && !kept.includes(activeFilePath)) {
        activeFilePath = path;
        void ensureLoaded(path);
      }
      persistWorkspace();
    },

    /** Set the active tab (must already be open). With autosave on, the file you're LEAVING is saved
     *  first if it has unsaved edits (IntelliJ saves on tab switch). Ensures the target's source is
     *  loaded — a tab restored from the workspace, or a foreign file, may not be cached yet — then
     *  persists. */
    async setActive(path: string) {
      if (!openFilePaths.includes(path)) return;
      const leaving = activeFilePath;
      if (leaving && leaving !== path && autosaveEnabled() && dirty.has(leaving)) {
        await saveText(leaving, sources.get(leaving) ?? '');
      }
      activeFilePath = path;
      await ensureLoaded(path);
      persistWorkspace();
      // Arriving at a tab is the moment its staleness matters: a clean buffer refreshes
      // itself before you read it, and a dirty one raises its conflict before you type into
      // a version that is about to lose. Fire-and-forget — never make a tab switch wait on I/O.
      void checkExternalChanges();
    },

    /** Update the cached source (editor edits route here): recompute dirty, schedule a debounced
     *  live re-index (so the BE index tracks the edit), and schedule a debounced autosave. */
    setSource(path: string, text: string) {
      sources.set(path, text);
      markDirty(path, text);
      scheduleReindex(path);
      scheduleAutosave(path);
    },

    /** Force an immediate live re-index of `path` (explicit save — flushes any
     *  pending debounce). No-op for demo/unloaded paths. */
    reindexNow(path: string) { reindexNow(path); },

    /** Ensure + return a file's current text (loads it if no tab is open). */
    loadText,
    /** Write `text` to a file: cache + disk + re-index. Used by save + rename apply. Returns
     *  whether it reached disk — `false` when the file changed underneath and the write was
     *  refused (a conflict is raised instead of an overwrite). */
    saveText,
    /** Save the active file's current buffer to disk. `false` when there is no active file —
     *  or when the save was **refused** because the file changed on disk (the conflict is
     *  raised instead), so the caller's "Saved" toast doesn't claim something that didn't
     *  happen. */
    async saveActive(): Promise<boolean> {
      const p = activeFilePath;
      if (!p) return false;
      return saveText(p, sources.get(p) ?? '');
    },

    // ── Dirty / autosave ──────────────────────────────────────────────────
    /** Whether `path` has unsaved edits (buffer differs from the last-saved snapshot). */
    isDirty(path: string): boolean { return dirty.has(path); },
    /** Every path with unsaved edits (reactive) — for a tab "modified" badge. */
    get dirtyPaths(): string[] { return [...dirty]; },
    /** Save every file with unsaved edits — the save-on-window-blur entry point (see BennuWindow).
     *  Ungated: the caller gates on the autosave setting. */
    saveAllDirty,

    // ── External changes ──────────────────────────────────────────────────
    /** Poll the open tabs for on-disk changes: adopt silently when the buffer is clean, raise
     *  a conflict when it isn't. Call on window focus, tab activation and a slow tick. */
    checkExternalChanges,
    /** Whether `path` changed on disk while its buffer had unsaved edits — neither side can be
     *  discarded without asking. Its autosave is suspended while this holds. */
    isConflicted(path: string): boolean { return conflicted.has(path); },
    /** Paths awaiting an external-change decision, restricted to the ones a tab is showing
     *  (reactive). The modal reads the first; the tab strip badges them.
     *
     *  Filtered to open tabs on purpose: the flag outlives a tab close (see `closeFile`), and
     *  a modal demanding a decision about a file with nothing on screen would be unanswerable.
     *  The suppression of autosave keys off the unfiltered set, so a closed-and-reopened
     *  conflict is still safe. */
    get conflictedPaths(): string[] {
      return openFilePaths.filter((p) => conflicted.has(p));
    },
    /** Resolve a conflict by **taking the version on disk**, discarding the buffer's edits. */
    async resolveTakeDisk(path: string) { await adoptFromDisk(path); },
    /** Resolve a conflict by **keeping the buffer**, overwriting what is on disk. The only
     *  route past the guard, and never automatic. */
    async resolveKeepMine(path: string) {
      await saveText(path, sources.get(path) ?? '', true);
    },
    // There is deliberately no "dismiss": a conflict is a fact about the file, and clearing
    // the flag without choosing would re-arm autosave into the very overwrite this prevents.
    // Deferring the decision is a *presentation* concern — the modal remembers what it has
    // already shown, while the tab badge keeps the file findable until a side is picked.
  };
}

export const projectStore = createProjectStore();

// MOCK — the sentinel demo root, re-exported for consumers that badge the demo.
export { DEMO_ROOT };
