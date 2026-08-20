/**
 * Bennu local-history store — what the Local History dialog is looking at.
 *
 * One store rather than component state because three different surfaces open the same
 * dialog (the project tree's context menu, the editor's, the Command Palette) and each
 * of them opens it *at* something: this file, this folder, the project, or the list of
 * files that are gone. Making that an argument to a component would mean every caller
 * knowing how the dialog is built.
 *
 * ## The scopes are one dialog, not four
 *
 * They answer four phrasings of the same question and share the timeline, the diff and
 * the actions — so they are a switch inside one window rather than four entry points.
 * The one that earns its place loudest is **Deleted**: a file that no longer exists has
 * no row anywhere to right-click, so a list that does not depend on the filesystem is
 * the only way to reach its history at all. That is the part IntelliJ makes you go
 * looking for, through the old folder's history, remembering the folder yourself.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  fileHistory as ipcFileHistory,
  folderHistory as ipcFolderHistory,
  deletedFiles as ipcDeleted,
  revisionDiff as ipcDiff,
  restoreRevision as ipcRestore,
  labelRevision as ipcLabel,
  type ChangeGroup,
  type DeletedEntry,
  type FolderEntry,
  type Revision,
  type TextDelta,
} from '$lib/ipc/bennu/history';

/** Which question the dialog is answering. */
export type HistoryScope = 'file' | 'folder' | 'project' | 'deleted';

/** What the diff pane is comparing — resolved from the current scope + selection, so
 *  every scope feeds the same viewer instead of each growing its own. */
interface DiffTarget {
  /** Absolute path of the file being compared. */
  file: string;
  /** The older side. */
  revision: string;
}

function createHistoryStore() {
  let open = $state(false);
  let scope = $state<HistoryScope>('file');
  let root = $state('');
  /** Absolute path of the file the `file` scope is about. */
  let file = $state('');
  /** Absolute path of the directory the `folder` scope is about. */
  let dir = $state('');

  let revisions = $state<Revision[]>([]);
  let timeline = $state<ChangeGroup[]>([]);
  let entries = $state<FolderEntry[]>([]);
  let deleted = $state<DeletedEntry[]>([]);

  let selected = $state<DiffTarget | null>(null);
  let delta = $state<TextDelta | null>(null);
  let loading = $state(false);
  let diffing = $state(false);
  let error = $state('');

  /** Guards against an out-of-order response overwriting a newer one — the scope switcher
   *  is a click away from the previous request landing.
   *
   *  TWO counters, deliberately: loading a scope *selects* a revision, so one shared
   *  counter would have the selection invalidate the load that made it — and the load
   *  would then never clear its own spinner. */
  let loadToken = 0;
  let diffToken = 0;

  /** Project-relative, forward slashes — what the backend keys history by, and what a
   *  row shows. */
  function rel(abs: string): string {
    const fwd = abs.replace(/\\/g, '/');
    const base = root.replace(/\\/g, '/').replace(/\/+$/, '');
    return fwd.startsWith(base + '/') ? fwd.slice(base.length + 1) : fwd;
  }

  function abs(relPath: string): string {
    const base = root.replace(/\\/g, '/').replace(/\/+$/, '');
    return relPath.startsWith(base) ? relPath : `${base}/${relPath}`;
  }

  /** Load whatever the current scope needs, and pick a first selection so the dialog
   *  opens showing something rather than an empty right-hand side. */
  async function load(): Promise<void> {
    if (!root) return;
    const mine = ++loadToken;
    loading = true;
    error = '';
    try {
      if (scope === 'file') {
        const h = await ipcFileHistory(root, file);
        if (mine !== loadToken) return;
        revisions = h.revisions;
        entries = [];
        timeline = [];
        // The newest revision is what is on disk; the interesting first comparison is
        // the one BEFORE it — "what did the last save change?" — so the second row is
        // selected when there is one.
        const first = h.revisions[1] ?? h.revisions[0];
        selectRevision(first ? { file, revision: first.id } : null);
      } else if (scope === 'deleted') {
        deleted = await ipcDeleted(root);
        if (mine !== loadToken) return;
        revisions = [];
        const first = deleted[0];
        selected = null;
        delta = null;
        if (first) void selectDeleted(first);
      } else {
        const target = scope === 'project' ? '' : dir;
        const h = await ipcFolderHistory(root, target);
        if (mine !== loadToken) return;
        entries = h.entries;
        timeline = h.timeline;
        revisions = [];
        const first = h.timeline[0]?.files[0];
        selectRevision(first ? { file: abs(first.path), revision: first.revision } : null);
      }
    } catch (e) {
      if (mine === loadToken) error = e instanceof Error ? e.message : String(e);
    } finally {
      if (mine === loadToken) loading = false;
    }
  }

  /** Compare `target` against what is on disk now — the comparison that answers "what
   *  would restoring this change?", which is the question the buttons below it act on. */
  function selectRevision(target: DiffTarget | null): void {
    selected = target;
    delta = null;
    if (!target) return;
    const mine = ++diffToken;
    diffing = true;
    void ipcDiff(root, target.file, target.revision)
      .then((d) => { if (mine === diffToken) delta = d; })
      .catch((e) => { if (mine === diffToken) error = e instanceof Error ? e.message : String(e); })
      .finally(() => { if (mine === diffToken) diffing = false; });
  }

  /** A deleted file has nothing on disk to compare against, so its whole last content is
   *  the answer — which the diff renders as one all-removed hunk, correctly. */
  async function selectDeleted(entry: DeletedEntry): Promise<void> {
    const target = abs(entry.path);
    const h = await ipcFileHistory(root, target);
    revisions = h.revisions;
    const last = h.revisions.find((r) => r.blob);
    selectRevision(last ? { file: target, revision: last.id } : null);
  }

  return {
    get open() { return open; },
    get scope() { return scope; },
    get root() { return root; },
    get file() { return file; },
    get dir() { return dir; },
    get revisions() { return revisions; },
    get timeline() { return timeline; },
    get entries() { return entries; },
    get deleted() { return deleted; },
    get selected() { return selected; },
    get delta() { return delta; },
    get loading() { return loading; },
    get diffing() { return diffing; },
    get error() { return error; },
    rel,

    /** The label of whatever the dialog is scoped to — the header's subtitle. */
    get subject() {
      if (scope === 'file') return rel(file);
      if (scope === 'folder') return rel(dir) || '.';
      if (scope === 'project') return root.split(/[\\/]/).pop() ?? root;
      return `${deleted.length} deleted`;
    },

    /** Open at a file. */
    show(projectRoot: string, target: string) {
      root = projectRoot;
      file = target;
      dir = target.replace(/[\\/][^\\/]*$/, '');
      scope = 'file';
      open = true;
      void load();
    },
    /** Open at a directory. */
    showFolder(projectRoot: string, target: string) {
      root = projectRoot;
      dir = target;
      file = '';
      scope = 'folder';
      open = true;
      void load();
    },
    /** Open at the whole project. */
    showProject(projectRoot: string) {
      root = projectRoot;
      dir = projectRoot;
      scope = 'project';
      open = true;
      void load();
    },
    /** Open at the files that are gone. */
    showDeleted(projectRoot: string) {
      root = projectRoot;
      scope = 'deleted';
      open = true;
      void load();
    },

    setScope(next: HistoryScope) {
      if (next === scope) return;
      // A scope with nothing to be about falls back to the project, rather than opening
      // empty and blaming the user for having right-clicked the wrong thing.
      if (next === 'file' && !file) return;
      scope = next;
      void load();
    },
    reload: load,
    select: selectRevision,
    selectDeleted,

    /** Put a revision back. Answers with where it landed, so the caller can open it. */
    async restore(target: string, revision?: string, to?: string): Promise<string> {
      const res = await ipcRestore(root, target, revision, to);
      await load();
      return res.file;
    },

    async label(target: string, revision: string, text: string): Promise<void> {
      await ipcLabel(root, target, revision, text);
      await load();
    },

    close() {
      open = false;
      revisions = [];
      timeline = [];
      entries = [];
      deleted = [];
      selected = null;
      delta = null;
      error = '';
    },
  };
}

export const bennuHistoryStore = createHistoryStore();
