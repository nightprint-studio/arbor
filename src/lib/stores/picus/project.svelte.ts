/**
 * Picus project — the script repository on disk: **the real directory tree**,
 * what each folder declares about itself, and the files inside.
 *
 * The structural invariant: **the dialect belongs to the folder**. Nothing here
 * exposes a "current dialect"; every consumer reads it off the folder it is
 * looking at.
 *
 * ## The tree is the tree
 *
 * There is no branch level and no invented grouping. A repository laid out as
 * `AGGIORNAMENTO/<version>/ORA` has its dialect five levels down and its role at
 * the top; another has it the other way round. One rule covers both: **any
 * directory may declare a dialect and/or a role, and everything below inherits
 * it until something overrides it**. `FolderNode` carries the declaration and
 * the inherited answer side by side, and this store additionally records *which*
 * ancestor each inherited answer came from — without that, "inherited" is a
 * dead end and the user cannot find the row to correct.
 *
 * ## Two ways to say what a folder is
 *
 * `classify()` answers for a **path**. `setAlias()` answers for a **name** —
 * every folder called `POS`, including the ones the next release will add — which
 * is the only shape that survives a repository shipping a folder set per
 * delivered version. Both write the same file and both replace the tree from the
 * reply, because only the backend knows the inheritance rule.
 *
 * ## Four engine states
 *
 * `unclassifiedFolders` is the folders **nobody could identify** — the only one
 * of the four that is a question. A folder in an engine Picus does not support
 * lives in `unsupportedFolders`, and a folder of **portable** SQL in
 * `genericFolders`; both are answers, and nothing in this store asks about them
 * again. A portable folder is also the one thing here that counts for more than
 * one dialect at a time.
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

import {
  type FolderAlias,
  type FolderEngine,
  type FolderNode,
  type FolderRole,
  type InventoryObject,
  type Project,
  type ScriptFile,
  engineIsUnknown,
  engineIsUnsupported,
  folderEngine,
  isGeneric,
} from '$lib/types/picus';
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
import {
  type ConfirmedProject,
  type FolderClassification,
  confirmProject,
  folderEdit,
  foldersNamed,
  setFolderAlias,
} from '$lib/ipc/picus/project';
import { consistencyStore } from './consistency.svelte';

/**
 * A folder plus everything the tree alone does not say: how deep it sits, who
 * its parent is, and **which ancestor its inherited dialect / role came from**.
 *
 * The provenance is the part that earns this shape. `effectiveEngine` says the
 * folder is Oracle; only `dialectFrom` says the declaration lives seven levels
 * up in `ORACLE/`, which is the row the user has to reach to change it.
 */
export interface FolderEntry {
  node: FolderNode;
  depth: number;
  parent: string | null;
  /** Path of the folder that declared the effective dialect; `null` when none did. */
  dialectFrom: string | null;
  /** Path of the folder that declared the effective role; `null` when none did. */
  roleFrom: string | null;
}

/** Below this many folders the tree is small enough to be worth opening whole. */
const OPEN_WHOLE_TREE_BELOW = 40;

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
  /**
   * Folder names this repository has declared a meaning for.
   *
   * Held here rather than fetched by whoever renders it because it *explains* the
   * tree: a `POS` folder reading as PostgreSQL when nothing about `POS` says so
   * is a mystery until the vocabulary is visible beside it.
   */
  let aliases = $state<FolderAlias[]>([]);
  let loading = $state(false);
  let error = $state('');
  /** A classification is being written into `.arbor/picus/project.toml`. */
  let classifying = $state(false);
  /**
   * Where the repository's own configuration was last written. A tool that
   * writes into someone's repository says where — the backend answers with the
   * path, and the confirmation quotes it.
   */
  let configPath = $state('');

  let inventory = $state<InventoryObject[]>([]);
  /** Indexed objects that belong to no folder — outside the model, not invisible. */
  let orphans = $state<ProjectNote[]>([]);
  let analyzing = $state(false);
  let analysisError = $state('');

  /** File text, keyed by project-relative path. Filled by `loadText`, read purely. */
  let texts = $state<Record<string, ScriptText>>({});
  let textErrors = $state<Record<string, string>>({});
  let textLoading = $state<Record<string, boolean>>({});

  /**
   * User overrides on top of {@link defaultExpanded}, keyed by folder path.
   * Absent means "whatever the default rule says"; the two are folded in
   * `expandedIds`, so a rescan that adds folders does not silently close the
   * ones the user opened.
   */
  let expandOverride = $state<Record<string, boolean>>({});

  const tree = $derived<FolderNode[]>(project?.tree ?? []);

  /**
   * Every folder, depth-first, with its depth, its parent and the provenance of
   * its inherited classification.
   *
   * Computed in one pass rather than by walking up from each row: the walk-up is
   * O(depth) per lookup and the tree is read on every render of every row.
   */
  const entries = $derived.by<FolderEntry[]>(() => {
    const out: FolderEntry[] = [];
    const walk = (
      node: FolderNode,
      depth: number,
      parent: string | null,
      dialectFrom: string | null,
      roleFrom: string | null,
    ) => {
      const ownDialect = node.engine !== null ? node.path : dialectFrom;
      const ownRole = node.role !== null ? node.path : roleFrom;
      out.push({ node, depth, parent, dialectFrom: ownDialect, roleFrom: ownRole });
      for (const child of node.children) walk(child, depth + 1, node.path, ownDialect, ownRole);
    };
    for (const node of tree) walk(node, 0, null, null, null);
    return out;
  });

  const byPath = $derived(new Map(entries.map((e) => [e.node.path, e])));

  /** Every file across the whole tree — the flat form searches and pickers want. */
  const allFiles = $derived<ScriptFile[]>(entries.flatMap((e) => e.node.files));

  /** Which file lives where, so a path resolves to its folder without a walk. */
  const folderOfPath = $derived.by(() => {
    const map = new Map<string, FolderEntry>();
    for (const e of entries) for (const f of e.node.files) map.set(f.path, e);
    return map;
  });

  /**
   * Which folders start open.
   *
   * A small repository opens whole — that is the two-branch case, and a tree that
   * starts folded hides the one thing the panel exists to show. A large one opens
   * only the paths that **lead somewhere classified**: with hundreds of version
   * folders, expanding everything is the same as showing nothing, while the
   * folders that declare an engine or a role are exactly the ones worth landing
   * on. The top level is always open, so the panel is never blank.
   */
  const defaultExpanded = $derived.by<Set<string>>(() => {
    const open = new Set<string>();
    if (!entries.length) return open;
    if (entries.length <= OPEN_WHOLE_TREE_BELOW) {
      for (const e of entries) open.add(e.node.path);
      return open;
    }
    for (const e of entries) if (e.depth === 0) open.add(e.node.path);
    for (const e of entries) {
      if (e.node.engine === null && e.node.role === null) continue;
      // The declaring folder itself, and everything between it and the root:
      // landing on the row is only useful if the path to it is visible.
      open.add(e.node.path);
      let parent = e.parent;
      while (parent) {
        open.add(parent);
        parent = byPath.get(parent)?.parent ?? null;
      }
    }
    return open;
  });

  const expandedIds = $derived.by<Set<string>>(() => {
    const open = new Set(defaultExpanded);
    for (const [path, wanted] of Object.entries(expandOverride)) {
      if (wanted) open.add(path);
      else open.delete(path);
    }
    return open;
  });

  /**
   * Folders that declare nothing anywhere up their chain — the ones to classify.
   *
   * A folder whose engine Picus does not support is **not** one of these. It has
   * an answer, and listing it here would put it in the "needs an answer" banner,
   * in the palette's per-folder entries and in the tree's warning icon — three
   * places asking a question the user settled the moment they said "SQL Server".
   */
  const unclassifiedFolders = $derived(
    entries.filter((e) => e.node.files.length > 0 && engineIsUnknown(e.node)),
  );

  /** Folders written in an engine Picus recognises and does not read. */
  const unsupportedFolders = $derived(
    entries.filter((e) => engineIsUnsupported(e.node) && e.node.files.length > 0),
  );

  /** Folders of portable SQL — written once, counted for every dialect. */
  const genericFolders = $derived(
    entries.filter((e) => isGeneric(e.node) && e.node.files.length > 0),
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
    aliases = res.aliases ?? [];
    error = '';
  }

  /** Take a write's reply: the tree, the vocabulary and the problems together. */
  function acceptWrite(res: ConfirmedProject) {
    project = res.project ?? project;
    aliases = res.aliases ?? [];
    problems = toNotes(res.problems);
    configPath = res.configPath ?? '';
    // The layout is now written down, so it is no longer a proposal.
    isNew = false;
    error = '';
    // Coverage and every consistency rule are stated in terms of engines and
    // roles; re-classifying anything invalidates the last verdict entirely.
    void analyze();
  }

  function forget() {
    project = null;
    notes = [];
    problems = [];
    isNew = false;
    aliases = [];
    configPath = '';
    inventory = [];
    orphans = [];
    analysisError = '';
    texts = {};
    textErrors = {};
    textLoading = {};
    expandOverride = {};
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
    /** The repository's directory hierarchy, as it is on disk. */
    get tree() { return tree; },
    /** Every folder, depth-first, with depth, parent and inheritance provenance. */
    get entries() { return entries; },
    get notes() { return notes; },
    get problems() { return problems; },
    get isNew() { return isNew; },
    get loading() { return loading; },
    get error() { return error; },
    get classifying() { return classifying; },
    /** Absolute path of the repository's own config, once something wrote it. */
    get configPath() { return configPath; },
    get inventory() { return inventory; },
    get orphans() { return orphans; },
    get analyzing() { return analyzing; },
    get analysisError() { return analysisError; },
    get allFiles() { return allFiles; },
    get fileCount() { return allFiles.length; },
    get folderCount() { return entries.length; },
    /** Folders holding files that no ancestor gave an engine to. */
    get unclassifiedFolders() { return unclassifiedFolders; },
    /** Folders holding files written in an engine Picus does not support. */
    get unsupportedFolders() { return unsupportedFolders; },
    /** Folders of portable SQL — one file that counts for every dialect. */
    get genericFolders() { return genericFolders; },
    /** Folder names this repository has declared a meaning for. */
    get aliases() { return aliases; },
    /** A repository is attached — whether or not it could be read. */
    get attached() { return root !== ''; },
    /** Questions the reader raised that nobody has answered. */
    get openQuestionCount() { return problems.filter((p) => p.needsAttention).length; },

    // ── The tree, and how much of it is open ──────────────────────────────────

    get expandedIds() { return expandedIds; },
    setFolderExpanded(path: string, open: boolean) {
      expandOverride = { ...expandOverride, [path]: open };
    },
    expandAll() {
      expandOverride = Object.fromEntries(entries.map((e) => [e.node.path, true]));
    },
    collapseAll() {
      expandOverride = Object.fromEntries(entries.map((e) => [e.node.path, false]));
    },
    /**
     * Open `path` and every ancestor, so the row can actually be seen.
     *
     * Called after a folder is classified from the dialog: the confirmation says
     * the write landed, and the tree then shows where.
     */
    revealFolder(path: string) {
      const next = { ...expandOverride };
      let cursor: string | null = path;
      while (cursor) {
        next[cursor] = true;
        cursor = byPath.get(cursor)?.parent ?? null;
      }
      expandOverride = next;
    },

    // ── Looking things up ─────────────────────────────────────────────────────

    entryFor(path: string): FolderEntry | null { return byPath.get(path) ?? null; },

    fileByPath(path: string): ScriptFile | null {
      return allFiles.find((f) => f.path === path) ?? null;
    },

    /** The folder a file sits in — the generator needs its role and its engine. */
    folderOfFile(path: string): FolderEntry | null {
      return folderOfPath.get(path) ?? null;
    },

    /**
     * The engine a file is written in: its folder's, after inheritance.
     *
     * `null` is an answer, not a failure — a file under a folder nothing has
     * classified has no engine, and every consumer renders that state rather
     * than guessing one. So is `generic`, which is why this answers with a
     * `FolderEngine` rather than a `Dialect`: a portable file has no single
     * dialect and pretending it had one is the lie the state exists to end.
     */
    dialectOfFile(path: string): FolderEngine | null {
      const folder = folderOfPath.get(path);
      return folder ? folderEngine(folder.node) : null;
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

    // ── Saying what a folder is ───────────────────────────────────────────────

    /**
     * Declare (or clear) a folder's engine and/or role, and save it with the
     * repository.
     *
     * Returns an empty string on success and the failure's own words otherwise —
     * the caller shows them where the action was taken. Passing `null` clears the
     * declaration so the folder inherits again; omitting a key leaves it alone.
     *
     * The reply's tree replaces this store's, rather than being patched into it:
     * a declaration changes what every descendant *effectively* is, and the
     * backend is the only thing that knows the inheritance rule.
     */
    async classify(path: string, change: FolderClassification): Promise<string> {
      if (!root || !path) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await confirmProject(root, [folderEdit(path, change)]));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    // ── Saying what a folder NAME means ───────────────────────────────────────

    /** What this project declares about a name, if anything. */
    aliasFor(name: string): FolderAlias | null {
      const key = name.trim().toLowerCase();
      return aliases.find((a) => a.name.trim().toLowerCase() === key) ?? null;
    },

    /**
     * Declare — or forget — what a folder name means in this repository.
     *
     * Both fields are replaced: an alias has exactly two, so there is no
     * "leave that one alone" state to encode. Passing neither removes it.
     *
     * Returns an empty string on success and the failure's own words otherwise,
     * exactly like `classify` — every caller shows them where the action was
     * taken.
     */
    async setAlias(
      name: string,
      engine: FolderEngine | null,
      role: FolderRole | null,
    ): Promise<string> {
      if (!root || !name.trim()) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await setFolderAlias(root, name.trim(), engine, role));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    /** Forget a name. The same write as clearing both of its fields. */
    async removeAlias(name: string): Promise<string> {
      return this.setAlias(name, null, null);
    },

    /**
     * Which folders an alias of this name would reach.
     *
     * Answered by the backend, by the same rule the alias itself will use — so
     * the count in the offer is the count the rule produces, not an approximation
     * of it computed here from folder names.
     */
    async foldersNamed(name: string): Promise<string[]> {
      if (!root || !name.trim()) return [];
      try {
        return await foldersNamed(root, name.trim());
      } catch {
        // The offer is an optimisation, never a blocker: if the count cannot be
        // had, the classification the user just made still stands.
        return [];
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
