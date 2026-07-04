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
    /** Encoding of the active file (defaults to `UTF-8`). */
    get activeEncoding() { return activeFilePath ? (encodings.get(activeFilePath) ?? 'UTF-8') : null; },
    /** Cached source of the active file (or '' when none / not yet loaded). */
    get activeSource()   { return activeFilePath ? (sources.get(activeFilePath) ?? '') : ''; },

    /** Cached source for `path` ('' when not loaded). */
    sourceOf(path: string): string { return sources.get(path) ?? ''; },
    /** Decoded encoding for `path` (defaults to `UTF-8`). */
    encodingOf(path: string): string { return encodings.get(path) ?? 'UTF-8'; },

    /** Open a project folder: resolve the manifest + file tree (no file opened).
     *  Tries real IPC; on backend-absent failure, falls back to the demo project
     *  so the shell stays populated. MOCK — drop the catch when bennu-be is live. */
    async openProject(dir: string) {
      try {
        const info = await ipcOpenProject(dir);
        // Show the project shell immediately, then load the (potentially large,
        // fully-recursive) file tree in the background — awaiting it here is what made
        // opening lag. A stale tree from a superseded open is dropped by the root guard.
        applyProject(info, null, false);
        void ipcProjectTree(info.root)
          .then((t) => { if (project?.root === canonPath(info.root)) tree = canonTree(t); })
          .catch(() => { /* leave the tree empty — the project still opened */ });
      } catch (err) {
        // MOCK — bennu-be not attached: fall back to the demo so opening the
        // window still shows a populated tree. Remove when the BE serves data.
        loadDemoProject();
        throw err; // let callers still observe the failure (they ignore it)
      }
    },

    /** MOCK — explicitly load the built-in demo project. Remove with the mock. */
    loadDemo() { loadDemoProject(); },

    /** Open a file as the active editor tab (loads its source + encoding if needed).
     *  Binary files are refused with a toast (opening one would choke the UTF-8 read
     *  — a `.xcf` once froze the window). */
    async openFile(path: string) {
      // Canonicalize (forward slashes) so a file opened via different callers — the project
      // tree (native `\`), a JSP include go-to or the class index (BE forward-slash) — keys
      // the SAME tab instead of a duplicate, and the active path matches the BE's
      // forward-slash file keys (e.g. the form-analysis include graph).
      path = canonPath(path);
      if (isBinaryPath(path)) {
        toastStore.show(`Can't open ${path.split(/[\\/]/).pop()} — binary file`, 'info');
        return;
      }
      await ensureLoaded(path);
      activeFilePath = path;
      if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
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
      }
    },

    /** Close every tab except `path` — it becomes the active one (so the surviving
     *  tab is always focused, regardless of what was active before). No-op when the
     *  path isn't open. */
    closeOthers(path: string) {
      if (!openFilePaths.includes(path)) return;
      openFilePaths = [path];
      activeFilePath = path;
    },

    /** Close all open tabs. */
    closeAll() {
      openFilePaths = [];
      activeFilePath = null;
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
      if (activeFilePath && !kept.includes(activeFilePath)) activeFilePath = path;
    },

    /** Set the active tab (must already be open). */
    setActive(path: string) {
      if (openFilePaths.includes(path)) activeFilePath = path;
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
