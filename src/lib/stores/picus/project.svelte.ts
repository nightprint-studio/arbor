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
 * ## Three ways to say what something is
 *
 * `classify()` answers for a folder **path**. `setAlias()` answers for a
 * **name** — every folder called `POS`, including the ones the next release will
 * add — which is the only shape that survives a repository shipping a folder set
 * per delivered version. `classifyFile()` answers for one **file**, which is
 * what an untidy repository needs: `4_12_ORA.sql` and `4_12_POS.sql` in one
 * directory that can say nothing true about either.
 *
 * All three write the same file and all three replace the tree from the reply,
 * because only the backend knows the resolution rule.
 *
 * `setExcluded()` answers a different question: not *what* something is, but
 * whether it is here at all. It is deliberately not a fourth role — `ignored`
 * says "not an installation folder" and its scripts are still read, indexed and
 * checked, while excluded says "pretend this is not in the repository". Merging
 * them would be fatal, because `ignored` is also the fallback for a folder
 * nobody has classified.
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
  type AliasScope,
  type FolderAlias,
  type FolderEngine,
  type FolderNode,
  type FolderRole,
  type InventoryObject,
  type Project,
  type ScriptFile,
  aliasScope,
  declaresExclusion,
  folderHasUnclassifiedScripts,
  engineIsUnsupported,
  fileDeclaresEngine,
  fileEngine,
  folderEngine,
  isExcluded,
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
  filesNamed,
  folderEdit,
  foldersNamed,
  // Aliased because the store method below has the same name: the property and
  // the import would read as each other at a glance, and this is a write.
  setExcluded as setExcludedOnDisk,
  setFileEngine,
  setFolderAlias,
  setFolderProduct,
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
   *
   * An **excluded** folder is closed either way. Its row stays visible — that row
   * is the only way to change one's mind — but what is inside it is, by the
   * user's own decision, not what this panel is for, and the folder somebody
   * excludes is typically the largest one in the repository. Opening it is one
   * keystroke on the row and the override is remembered, so the script that needs
   * rescuing is never out of reach.
   */
  const defaultExpanded = $derived.by<Set<string>>(() => {
    const open = new Set<string>();
    if (!entries.length) return open;
    if (entries.length <= OPEN_WHOLE_TREE_BELOW) {
      for (const e of entries) open.add(e.node.path);
    } else {
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
    }
    for (const e of entries) if (isExcluded(e.node)) open.delete(e.node.path);
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
   * Folders that still contain a script with no engine — the ones to classify.
   *
   * A folder whose engine Picus does not support is **not** one of these. It has
   * an answer, and listing it here would put it in the "needs an answer" banner,
   * in the palette's per-folder entries and in the tree's warning icon — three
   * places asking a question the user settled the moment they said "SQL Server".
   *
   * Nor is a folder whose own engine is unknown but whose **files** all answer
   * for themselves. That is the shape of an untidy repository — `4_12_ORA.sql`
   * beside `4_12_POS.sql`, told apart by the file names — and the folder above
   * them is not one thing and never will be. Once every script in it has an
   * engine there is no question left, and asking about the container anyway is
   * asking for an answer that would change nothing.
   */
  const unclassifiedFolders = $derived(
    entries.filter((e) => !isExcluded(e.node) && folderHasUnclassifiedScripts(e.node)),
  );

  /** Folders written in an engine Picus recognises and does not read. */
  const unsupportedFolders = $derived(
    entries.filter((e) => engineIsUnsupported(e.node) && e.node.files.length > 0 && !isExcluded(e.node)),
  );

  /** Folders of portable SQL — written once, counted for every dialect. */
  const genericFolders = $derived(
    entries.filter((e) => isGeneric(e.node) && e.node.files.length > 0 && !isExcluded(e.node)),
  );

  /**
   * Folders and scripts that **declare** they are outside the project.
   *
   * Only the declaring rows, never the ones merely inheriting: one excluded
   * folder would otherwise enumerate its whole subtree, and putting a
   * descendant "back" is not what the user means — the decision lives on the
   * folder that made it. A short list by construction, which is what makes it
   * safe to address one by one from the palette. Without that, a repository
   * whose excluded folders are all collapsed has nowhere to say "actually, no".
   */
  const excludedFolders = $derived(
    entries.filter((e) => declaresExclusion(e.node) && isExcluded(e.node)),
  );

  /** The same, one level down: scripts excluded by their own declaration. */
  const excludedFiles = $derived(
    allFiles.filter((f) => declaresExclusion(f) && isExcluded(f)),
  );

  /**
   * Files that answer for themselves instead of taking their folder's word.
   *
   * A short list by construction — a tidy repository has none — and the only
   * files worth enumerating anywhere: they are the ones carrying an answer the
   * folder header does not, and therefore the ones somebody may want to revisit
   * or clear. Every *other* file is reachable through a filter, which is where a
   * per-file entry would only have been noise.
   */
  const declaredFiles = $derived(
    allFiles.filter((f) => fileDeclaresEngine(f) && !isExcluded(f)),
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
    /** Files that declare their own engine rather than inheriting their folder's. */
    get declaredFiles() { return declaredFiles; },
    /** Folders declared out of the project — the rows that can put themselves back. */
    get excludedFolders() { return excludedFolders; },
    /** Scripts declared out of the project, one by one. */
    get excludedFiles() { return excludedFiles; },
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

    /**
     * Open the folder holding a file, so the file's row can be seen.
     *
     * The same courtesy {@link revealFolder} pays a classified folder: after a
     * file is classified from a dialog or the palette, the chip that appeared on
     * its row is the confirmation, and a confirmation nobody can see is a claim.
     */
    revealFile(path: string) {
      const folder = folderOfPath.get(path);
      if (folder) this.revealFolder(folder.node.path);
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
     * The engine a file is written in — **the file's own answer**, after it has
     * fallen back to its folder.
     *
     * Asked of the file rather than of the folder, which is the same answer for
     * all but the handful of files that say otherwise. A caller that asked the
     * folder would be right almost always, and wrong exactly where file-level
     * classification exists: the directory holding `4_12_ORA.sql` beside
     * `4_12_POS.sql` has no engine of its own, and both of its files do.
     *
     * `null` is an answer, not a failure — a file nothing has classified, here
     * or above, has no engine, and every consumer renders that state rather than
     * guessing one. So is `generic`, which is why this answers with a
     * `FolderEngine` rather than a `Dialect`: a portable file has no single
     * dialect and pretending it had one is the lie the state exists to end.
     */
    dialectOfFile(path: string): FolderEngine | null {
      const file = allFiles.find((f) => f.path === path);
      if (file) return fileEngine(file);
      // A path that is not in the tree still has a folder often enough to be
      // worth answering for — a destination picked before a rescan, say.
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

    /**
     * Say which installed product a folder's scripts belong to — or clear it.
     *
     * Separate from `classify` rather than a third field on it, because it is a
     * different question with a different answer type: engine and role are closed
     * sets Picus knows, a product name is the repository's own vocabulary.
     * `null` clears the declaration and the folder inherits again.
     *
     * Only meaningful for a repository that installs more than one product into
     * one version table — see `ProjectSettings.products`.
     */
    async setProduct(path: string, product: string | null): Promise<string> {
      if (!root) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await setFolderProduct(root, path, product));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    // ── Saying what ONE FILE is ───────────────────────────────────────────────

    /**
     * Declare (or clear) the engine of a single file, and save it with the
     * repository.
     *
     * The leaf of the same chain `classify` and `setAlias` sit on, for the
     * repositories where the folder cannot answer: `4_12_ORA.sql` beside
     * `4_12_POS.sql` in one directory. `null` clears the declaration and the
     * file follows its folder again.
     *
     * Returns an empty string on success and the failure's own words otherwise,
     * exactly like `classify` — every caller shows them where the action was
     * taken. The reply's tree replaces this store's for the same reason too: a
     * file declaration changes which lane the file counts in, and the backend is
     * the only thing that knows the resolution rule.
     */
    async classifyFile(path: string, engine: FolderEngine | null): Promise<string> {
      if (!root || !path) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await setFileEngine(root, path, engine));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    // ── Saying what is NOT ours to look at ────────────────────────────────────

    /**
     * Take a folder or a script out of the project — or put it back.
     *
     * One method for both, like the verb behind it: the path names whichever it
     * is, and it is one decision about one row. Excluding is **not** the
     * `ignored` role — an ignored folder is still read, indexed and checked and
     * simply is not an installation folder, while an excluded one is treated as
     * though it were not in the repository at all.
     *
     * `false` on a **file** is not the no-op it looks like: it rescues that one
     * script from an excluded folder, which is the only way to keep the single
     * migration that does matter without moving it on disk.
     *
     * Returns an empty string on success and the failure's own words otherwise,
     * exactly like `classify` — every caller shows them where the action was
     * taken. The reply's tree replaces this store's for the same reason too:
     * exclusion inherits, so it changes what every descendant effectively is.
     */
    async setExcluded(path: string, excluded: boolean): Promise<string> {
      if (!root) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await setExcludedOnDisk(root, path, excluded));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    // ── Saying what a NAME means ──────────────────────────────────────────────

    /** What this project declares about a name, if anything. */
    aliasFor(name: string): FolderAlias | null {
      const key = name.trim().toLowerCase();
      return aliases.find((a) => a.name.trim().toLowerCase() === key) ?? null;
    },

    /**
     * Where an existing alias of this name applies — the default when there is
     * none.
     *
     * Exists so a caller that only means to change an engine can send the scope
     * the alias already has: every field of an alias is *replaced*, so omitting
     * the scope would quietly move a file-matching rule back to folders only.
     */
    aliasScopeFor(name: string): AliasScope {
      const existing = this.aliasFor(name);
      return existing ? aliasScope(existing) : 'folders';
    },

    /**
     * Declare — or forget — what a name means in this repository.
     *
     * Every field is replaced: an alias has exactly these three, so there is no
     * "leave that one alone" state to encode. Which makes `appliesTo` required
     * rather than optional — omitting it would not keep what the alias said, it
     * would move a file-matching rule back to folders only. Callers that are
     * editing something else pass `aliasScopeFor(name)`.
     *
     * Passing neither an engine nor a role removes it.
     *
     * Returns an empty string on success and the failure's own words otherwise,
     * exactly like `classify` — every caller shows them where the action was
     * taken.
     */
    async setAlias(
      name: string,
      engine: FolderEngine | null,
      role: FolderRole | null,
      appliesTo: AliasScope,
    ): Promise<string> {
      if (!root || !name.trim()) return 'No repository is attached.';
      classifying = true;
      try {
        acceptWrite(await setFolderAlias(root, name.trim(), engine, role, appliesTo));
        return '';
      } catch (e) {
        return String(e);
      } finally {
        classifying = false;
      }
    },

    /** Forget a name. The same write as clearing every one of its fields. */
    async removeAlias(name: string): Promise<string> {
      return this.setAlias(name, null, null, 'folders');
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

    /**
     * Every **file** an alias of this name would reach — the same question, one
     * level down, answered by the same rule for the same reason.
     */
    async filesNamed(name: string): Promise<string[]> {
      if (!root || !name.trim()) return [];
      try {
        return await filesNamed(root, name.trim());
      } catch {
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
