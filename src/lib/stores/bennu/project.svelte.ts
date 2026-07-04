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

import { SvelteMap } from 'svelte/reactivity';
import {
  openProject as ipcOpenProject,
  projectTree as ipcProjectTree,
  readFile as ipcReadFile,
  writeFile as ipcWriteFile,
} from '$lib/ipc/bennu';
// Live re-index — kept in its own IPC file to avoid racing edits on index.ts.
import { didChange as ipcDidChange } from '$lib/ipc/bennu/nav';
// Workspace store owns the SET of named workspaces + persistence; this store is the live runtime
// of the ACTIVE workspace only. It reports its session snapshot up on every change (see
// `persistWorkspace`). Circular import is safe: neither store touches the other at construction.
import { workspacesStore } from './workspaces.svelte';
import type { ProjectSession } from '$lib/ipc/bennu/config';
import type { ProjectInfo, TreeNode } from '$lib/types/bennu';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
// MOCK — remove when bennu-be serves real data.
import { DEMO_PROJECT, DEMO_TREE, DEMO_ROOT, isDemoPath, demoReadFile } from './bennu-mock';

/** Extensions Bennu refuses to open in the text editor: opening a large binary would
 *  make `bennu_read_file` (UTF-8 decode) choke — a `.xcf` once froze the window. The
 *  guard is by extension (cheap, no read); a proper binary preview is a later wave. */
const BINARY_EXTENSIONS = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'bmp', 'ico', 'webp', 'svgz', 'xcf', 'psd', 'ai',
  'pdf', 'zip', 'jar', 'war', 'ear', 'class', 'exe', 'dll', 'so', 'dylib', 'bin',
  'o', 'obj', 'a', 'lib', '7z', 'gz', 'bz2', 'xz', 'tar', 'rar', 'iso', 'dmg',
  'mp3', 'mp4', 'm4a', 'wav', 'flac', 'ogg', 'avi', 'mov', 'mkv', 'webm',
  'ttf', 'otf', 'woff', 'woff2', 'eot', 'db', 'sqlite', 'mdb', 'keystore', 'jks',
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
  let activeFilePath = $state<string | null>(null);
  let openFilePaths = $state<string[]>([]);

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

  /** Build the active workspace's session snapshot from the live flat state (active project) +
   *  the stashed sessions (the other members). Pure — used both to persist and to flush the live
   *  state into the workspace store before a workspace switch. Empty for the demo / no project. */
  function snapshotSession(): { active_project: string; projects: ProjectSession[] } {
    const activeRoot = project?.root;
    if (!activeRoot || isDemo) return { active_project: '', projects: [] };
    // The ACTIVE root comes from the live flat state, the rest from their stashed sessions.
    // `workspaceRoots` always includes the active root.
    const roots = workspaceRoots.includes(activeRoot) ? workspaceRoots : [activeRoot];
    const projects = roots.map((r) => {
      if (r === activeRoot) {
        return { root: r, open_files: openFilePaths, active_file: activeFilePath ?? '' };
      }
      const s = sessions.get(r);
      return { root: r, open_files: s?.openFilePaths ?? [], active_file: s?.activeFilePath ?? '' };
    });
    return { active_project: activeRoot, projects };
  }

  // Persist the active workspace's session (open tabs + active tab per project) debounced, so the
  // next launch restores it. Routes THROUGH the workspace store (which owns the named-workspace
  // list + the `workspace.toml` write). Never persists the demo or a null project; a burst of tab
  // opens/closes coalesces into one write.
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  function persistWorkspace() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      if (!project || isDemo) return;
      workspacesStore.saveActiveSession(snapshotSession());
    }, 300);
  }

  /** Open a file as the active tab (loads source + encoding if needed), refusing binaries.
   *  The persistence-free core, shared by the public {@link openFile} (which persists after)
   *  and boot-time restore (which persists once at the end). */
  async function openFileInternal(path: string) {
    path = canonPath(path);
    if (isBinaryPath(path)) {
      toastStore.show(`Can't open ${path.split(/[\\/]/).pop()} — binary file`, 'info');
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
      encodings.set(path, res.encoding);
      return;
    }
    try {
      const res = await ipcReadFile(project?.root ?? path, path);
      sources.set(path, res.text);
      encodings.set(path, res.encoding);
    } catch {
      sources.set(path, '');
      encodings.set(path, 'utf-8');
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
    // Drop any pending re-index timers from the previous project.
    for (const t of reindexTimers.values()) clearTimeout(t);
    reindexTimers.clear();
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

  /** Write `text` to `path`: update the cache, persist to disk (`bennu_write_file`,
   *  skipped for demo/BE-absent), and flush a live re-index. The editor's controlled
   *  `value` re-syncs from the updated cache, so an open buffer reflects the change. */
  async function saveText(path: string, text: string): Promise<void> {
    sources.set(path, text);
    if (!isDemoPath(path)) {
      try {
        const res = await ipcWriteFile(project?.root ?? path, path, text);
        encodings.set(path, res.encoding);
      } catch {
        /* BE absent — cache updated, disk not (best-effort) */
      }
    }
    reindexNow(path);
  }

  return {
    get project()        { return project; },
    get tree()           { return tree; },
    get capabilities()   { return project?.capabilities ?? null; },
    get activeFilePath() { return activeFilePath; },
    get openFilePaths()  { return openFilePaths; },
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
      for (const p of projects) {
        let info;
        // eslint-disable-next-line no-await-in-loop
        try { info = await ipcOpenProject(p.root); } catch { continue; } // a project that's gone
        const root = canonPath(info.root);
        sessions.set(root, {
          info: { ...info, root },
          tree: null,
          openFilePaths: p.open_files.map(canonPath),
          activeFilePath: p.active_file ? canonPath(p.active_file) : null,
        });
        workspaceRoots = [...workspaceRoots, root];
        rememberRecent(root);
        loadTreeInto(root);
      }
      if (!workspaceRoots.length) return; // nothing opened (all gone / BE down)
      // Activate the remembered active project (or the first that opened), then load its active file.
      const wantActive = canonPath(active);
      const target = workspaceRoots.includes(wantActive) ? wantActive : workspaceRoots[0];
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

    /** Close a tab; pick a neighbour as active. */
    closeFile(path: string) {
      const idx = openFilePaths.indexOf(path);
      if (idx === -1) return;
      openFilePaths = openFilePaths.filter((p) => p !== path);
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

    /** Set the active tab (must already be open). Ensures its source is loaded — a tab restored
     *  from the workspace, or a foreign file, may not be cached yet — then persists. */
    async setActive(path: string) {
      if (!openFilePaths.includes(path)) return;
      activeFilePath = path;
      await ensureLoaded(path);
      persistWorkspace();
    },

    /** Update the cached source (editor edits route here) and schedule a debounced
     *  live re-index so the BE index tracks the edit without reopening the project. */
    setSource(path: string, text: string) {
      sources.set(path, text);
      scheduleReindex(path);
    },

    /** Force an immediate live re-index of `path` (explicit save — flushes any
     *  pending debounce). No-op for demo/unloaded paths. */
    reindexNow(path: string) { reindexNow(path); },

    /** Ensure + return a file's current text (loads it if no tab is open). */
    loadText,
    /** Write `text` to a file: cache + disk + re-index. Used by save + rename apply. */
    saveText,
    /** Save the active file's current buffer to disk. Returns false when no file is
     *  active. */
    async saveActive(): Promise<boolean> {
      const p = activeFilePath;
      if (!p) return false;
      await saveText(p, sources.get(p) ?? '');
      return true;
    },
  };
}

export const projectStore = createProjectStore();

// MOCK — the sentinel demo root, re-exported for consumers that badge the demo.
export { DEMO_ROOT };
