/**
 * Picus project — the script repository on disk: per-dialect branches, their
 * folders (each with a role), and the files inside.
 *
 * The structural invariant: **the dialect belongs to the folder**. Nothing here
 * exposes a "current dialect"; every consumer reads it off the branch it is
 * looking at.
 *
 * What a live database contains is NOT here — that is `schema.svelte.ts`. The
 * project is what is on disk; the schema is what a connection reports.
 *
 * ## A repository belongs to a connection
 *
 * Picus is database-oriented: you open a database, and *its* scripts are what you
 * see. The root therefore arrives from whichever connection is active
 * (`connectionsStore.activeScriptRoot`) rather than from a global "open project"
 * — the window's `$effect` calls `open()` when that changes. A connection with no
 * root attached leaves this store closed, which is a normal state and not an
 * error.
 *
 * ## Opening and checking are two round trips
 *
 * `open()` awaits the tree — fast enough to draw a panel on — and then kicks the
 * analysis off **without awaiting it**. The tree appears immediately and the
 * verdict fills in behind it; nothing blocks. Both are sequence-guarded, so
 * switching connection twice quickly never lets the first answer overwrite the
 * second.
 *
 * `analyze()` produces two stores' worth of result: the inventory belongs here,
 * the findings belong to `consistencyStore`. This store owns the call and pushes
 * to that one, which keeps the import graph a line instead of a cycle and keeps
 * "when was this last checked" a single fact.
 */

import type { Branch, Dialect, InventoryObject, Project, ScriptFile } from '$lib/types/picus';
import {
  type OpenScriptsResult,
  type ProjectNote,
  type ScriptText,
  analyzeScripts,
  openScripts,
  refreshScripts,
  scriptText,
  toNotes,
} from '$lib/ipc/picus/scripts';
import { consistencyStore } from './consistency.svelte';

function createProjectStore() {
  /** Absolute path of the attached repository; empty when none is. */
  let root = $state('');
  let project = $state<Project | null>(null);
  /** What the reader inferred and states out loud. */
  let notes = $state<ProjectNote[]>([]);
  /** What the reader could not settle — a question, not a footnote. */
  let problems = $state<ProjectNote[]>([]);
  /** The repository has no saved `.arbor/picus/project.toml`: this layout is a guess. */
  let isNew = $state(false);
  let loading = $state(false);
  let error = $state('');

  let inventory = $state<InventoryObject[]>([]);
  /** Indexed objects that belong to no branch — outside the model, not invisible. */
  let orphans = $state<ProjectNote[]>([]);
  let analyzing = $state(false);
  let analysisError = $state('');

  /** File text, keyed by project-relative path. Filled by `loadText`, read purely. */
  let texts = $state<Record<string, ScriptText>>({});
  let textErrors = $state<Record<string, string>>({});
  let textLoading = $state<Record<string, boolean>>({});

  /**
   * Tree expansion, keyed by branch/folder id. Absent means **open**: a Picus
   * repository is two branches of three folders, and a tree that starts folded
   * hides the one thing the panel exists to show.
   */
  let collapsed = $state<Record<string, boolean>>({});

  const branches = $derived<Branch[]>(project?.branches ?? []);

  /** Every file across every branch — the flat form searches and pickers want. */
  const allFiles = $derived<ScriptFile[]>(
    branches.flatMap((b) => b.folders.flatMap((f) => f.files)),
  );

  // Only the newest read may write: switching connection twice in a second must
  // not let the first repository's tree land on top of the second's.
  let openSeq = 0;
  let analyzeSeq = 0;

  function accept(res: OpenScriptsResult) {
    project = res.project ?? null;
    notes = res.notes ?? [];
    problems = toNotes(res.problems);
    isNew = !!res.isNew;
    error = '';
  }

  function forget() {
    project = null;
    notes = [];
    problems = [];
    isNew = false;
    inventory = [];
    orphans = [];
    analysisError = '';
    texts = {};
    textErrors = {};
    textLoading = {};
    collapsed = {};
    consistencyStore.clear();
  }

  /** Run the rules. Never awaited by `open` — the whole point is that it doesn't block. */
  async function analyze(): Promise<void> {
    if (!root) return;
    const seq = ++analyzeSeq;
    const forRoot = root;
    analyzing = true;
    analysisError = '';
    consistencyStore.beginRun();
    try {
      const res = await analyzeScripts(forRoot);
      if (seq !== analyzeSeq) return;
      inventory = res.inventory ?? [];
      orphans = toNotes(res.orphans, false);
      consistencyStore.acceptAnalysis(res);
    } catch (e) {
      if (seq !== analyzeSeq) return;
      inventory = [];
      orphans = [];
      analysisError = String(e);
      consistencyStore.failRun(String(e));
    } finally {
      if (seq === analyzeSeq) analyzing = false;
    }
  }

  async function read(nextRoot: string, force: boolean): Promise<void> {
    const seq = ++openSeq;
    root = nextRoot;
    loading = true;
    error = '';
    try {
      const res = force ? await refreshScripts(nextRoot) : await openScripts(nextRoot);
      if (seq !== openSeq) return;
      accept(res);
      // A re-read invalidates every buffer that came from the previous one.
      texts = {};
      textErrors = {};
    } catch (e) {
      if (seq !== openSeq) return;
      project = null;
      notes = [];
      problems = [];
      isNew = false;
      error = String(e);
    } finally {
      if (seq === openSeq) loading = false;
    }
    if (seq !== openSeq || !project) return;
    void analyze();
  }

  return {
    get root() { return root; },
    get project() { return project; },
    get branches() { return branches; },
    get notes() { return notes; },
    get problems() { return problems; },
    get isNew() { return isNew; },
    get loading() { return loading; },
    get error() { return error; },
    get inventory() { return inventory; },
    get orphans() { return orphans; },
    get analyzing() { return analyzing; },
    get analysisError() { return analysisError; },
    get allFiles() { return allFiles; },
    get fileCount() { return allFiles.length; },
    /** A repository is attached — whether or not it could be read. */
    get attached() { return root !== ''; },
    /** Questions the reader raised that nobody has answered. */
    get openQuestionCount() { return problems.filter((p) => p.needsAttention).length; },

    isExpanded(id: string) { return !collapsed[id]; },
    toggle(id: string) { collapsed = { ...collapsed, [id]: !collapsed[id] }; },
    setExpanded(id: string, open: boolean) { collapsed = { ...collapsed, [id]: !open }; },

    fileByPath(path: string): ScriptFile | null {
      return allFiles.find((f) => f.path === path) ?? null;
    },

    /** Which branch a project-relative path belongs to — and therefore its dialect. */
    branchOfFile(path: string): Branch | null {
      return branches.find((b) => b.folders.some((f) => f.files.some((x) => x.path === path))) ?? null;
    },

    dialectOfFile(path: string): Dialect | null {
      return this.branchOfFile(path)?.dialect ?? null;
    },

    /** The folder a path sits in — the generator needs its role. */
    folderOfFile(path: string) {
      for (const b of branches) {
        const folder = b.folders.find((f) => f.files.some((x) => x.path === path));
        if (folder) return { branch: b, folder };
      }
      return null;
    },

    // ── File text ─────────────────────────────────────────────────────────────

    /** A **pure read** of the text cache — safe from markup and from a `$derived`. */
    textFor(path: string): ScriptText | null {
      return texts[path] ?? null;
    },
    textErrorFor(path: string): string { return textErrors[path] ?? ''; },
    isTextLoading(path: string): boolean { return !!textLoading[path]; },

    /**
     * Fetch a file's text once. Call from an event handler or an `$effect` — it
     * writes the cache, which is why `textFor` and it are two different things.
     */
    async loadText(path: string, force = false): Promise<void> {
      if (!root || !path) return;
      if (!force && (texts[path] || textLoading[path])) return;
      textLoading = { ...textLoading, [path]: true };
      try {
        const res = await scriptText(root, path);
        texts = { ...texts, [path]: res };
        const { [path]: _dropped, ...rest } = textErrors;
        textErrors = rest;
      } catch (e) {
        textErrors = { ...textErrors, [path]: String(e) };
      } finally {
        textLoading = { ...textLoading, [path]: false };
      }
    },

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /** Attach and read a repository. A no-op when it is already the open one. */
    async open(nextRoot: string): Promise<void> {
      if (!nextRoot) { this.close(); return; }
      if (nextRoot === root && project) return;
      forget();
      await read(nextRoot, false);
    },

    /** Re-read from disk — the explicit "I changed files outside Picus". */
    async refresh(): Promise<void> {
      if (!root) return;
      await read(root, true);
    },

    /** Re-run the rules without re-reading the tree. */
    analyze,

    /** No connection, or one with no repository attached. */
    close() {
      openSeq++;
      analyzeSeq++;
      root = '';
      loading = false;
      analyzing = false;
      error = '';
      forget();
    },
  };
}

export const picusProjectStore = createProjectStore();
