/**
 * Picus project IPC — agreeing with the backend on what a repository *is*.
 *
 * Reading a repository (`ipc/picus/scripts.ts`) answers "what is in this folder".
 * This file answers the other half: "and which of it is Oracle, which is an
 * update script, and who says so". Discovery proposes; **nothing is written into
 * someone's repository until this call is made**, and when it is, the backend
 * says where it wrote (`configPath`).
 *
 * ## Why an edit is not a folder
 *
 * A correction travels as `{ path, dialect?, role? }` rather than as a whole
 * folder, because the three states of each field carry the whole meaning:
 *
 *  | wire            | means                                   |
 *  |-----------------|-----------------------------------------|
 *  | field absent    | leave it exactly as it is               |
 *  | field `null`    | clear the declaration — inherit again    |
 *  | field set       | declare it here                          |
 *
 * `undefined` and `null` are therefore **not interchangeable** here, and
 * `JSON.stringify` drops the former while keeping the latter — which is exactly
 * the distinction the backend deserialises. Build edits with {@link folderEdit}
 * rather than by hand, so an accidental `undefined` never becomes a silent no-op.
 */

import type {
  AliasScope,
  Dialect,
  FolderAlias,
  FolderEngine,
  FolderRole,
  ForeignEngine,
  Project,
} from '$lib/types/picus';
import { picus } from '../rpc';

/**
 * One correction to one folder, keyed by its project-relative path.
 *
 * Keyed by path rather than by an id because a path is what the user sees and
 * what survives a rescan.
 */
export interface ProjectEdit {
  path: string;
  /**
   * Absent = leave alone · `null` = back to inherited · set = declare here.
   *
   * May name an engine Picus does not read (`'sqlserver'`): a folder has one
   * engine, so it travels in one key.
   */
  dialect?: FolderEngine | null;
  /** Absent = leave alone · `null` = back to inherited · set = declare here. */
  role?: FolderRole | null;
}

/** What the user asked to change about a folder. Absent keys are not sent. */
export interface FolderClassification {
  dialect?: FolderEngine | null;
  role?: FolderRole | null;
}

/**
 * Build an edit that carries exactly the fields that were asked for.
 *
 * Written out rather than spread from the caller's object so an `undefined`
 * value can never travel as a present-but-empty key: `{ dialect: undefined }`
 * survives object spread and would read as "leave alone" only by accident.
 */
export function folderEdit(path: string, change: FolderClassification): ProjectEdit {
  const edit: ProjectEdit = { path };
  if ('dialect' in change) edit.dialect = change.dialect ?? null;
  if ('role' in change) edit.role = change.role ?? null;
  return edit;
}

export interface ConfirmedProject {
  /** Absolute path of the file that was written — a tool writing into your
   *  repository should say where. */
  configPath: string;
  /** The tree as it stands after the corrections, ready to replace the old one. */
  project: Project;
  /** The project's folder-name vocabulary as it stands after them. */
  aliases: FolderAlias[];
  /** What is wrong with the configuration now — reported, never fatal. */
  problems: string[];
}

/**
 * Apply corrections and write `.arbor/picus/project.toml`.
 *
 * The backend re-reads the folder rather than trusting a client-held snapshot,
 * and answers with the tree as it will look from now on — so the caller replaces
 * its project with this reply instead of patching its own copy.
 */
export function confirmProject(root: string, edits: ProjectEdit[]): Promise<ConfirmedProject> {
  return picus('picus_confirm_project', { root, edits });
}

/**
 * The project-wide settings — everything that is a fact about **the repository**
 * rather than about this machine.
 *
 * They live in `.arbor/picus/project.toml`, which is committed: a colleague
 * opening the same folder inherits the version table, the expected encoding and
 * which rules the team decided not to run. Putting any of these in the profile
 * would make the same scripts judged differently per person, which is the class
 * of surprise Picus exists to remove.
 */
export interface ProjectSettings {
  /** The encoding folders are expected to be in unless one overrides it. */
  encoding: string;
  eol: 'CRLF' | 'LF';
  /** **Empty switches the version guards off** — and the report says so. */
  versionTable: string;
  versionColumn: string;
  /** Empty means the project stamps no date; the closing UPDATE leaves it out. */
  dateColumn: string;
  /** Extra predicate, for a version table holding one row per module. */
  versionFilter: string;
  /**
   * Other tables that also record a version in this repository.
   *
   * Names only. A repository installing more than one product has a version
   * table per module, and an update script belonging to the second module
   * guards against the second table — perfectly correctly. These satisfy the
   * guard rules; generation still stamps the primary, because something has to
   * be stamped.
   */
  otherVersionTables: string[];
  /** What the initialisation folders are, relative to the updates. */
  initialisation: InitialisationModel;
  /**
   * Compare one dialect's scripts against the other's at all.
   *
   * On by default — it is what Picus is for. Off for a repository whose two
   * halves have diverged far enough that the comparison says nothing usable;
   * the version chain, the duplicates, the dangerous DML and the encodings are
   * worth having on their own.
   */
  compareDialects: boolean;
  /** Rule ids this repository does not want run, e.g. `['CONS001']`. */
  disabledRules: string[];
  /**
   * Object names the rules say nothing about.
   *
   * The escape hatch for the handful of tables in every real repository that are
   * a special case for a reason nothing in the scripts can express. Matched on
   * the name, case-insensitively, whatever kind of object carries it — and it
   * excludes them from the **rules**, not from the index: they still appear in
   * the Inventory with their coverage.
   */
  excludedObjects: string[];
  /**
   * The products this repository installs, when it installs more than one.
   *
   * Empty for the ordinary repository. This declares what a product **is** — the
   * predicate that selects its row of the version table; {@link setFolderProduct}
   * declares where its scripts **are**.
   */
  products: ProductSetting[];
}

/** One installed product, and the predicate that selects its version row. */
export interface ProductSetting {
  /** What folders name to say they belong here. Matched case-insensitively. */
  name: string;
  /** `MODULO = 'PORTALE'`. Empty means this product's table holds one row. */
  versionFilter: string;
}

/**
 * How a repository's initialisation folders relate to its update folders.
 *
 * Not derivable from the SQL — it is a fact about how the team works — and the
 * two propagation rules each only make sense under one reading of it.
 */
export type InitialisationModel =
  /** Kept at the latest version: it holds rows no update carries, and that is
   *  correct. Only "an update adds a row the initialisation never seeds" is
   *  reported. */
  | 'cumulative'
  /** Two accounts of the same changes, which must agree in both directions. */
  | 'mirrored'
  /** Maintained separately; comparing them says nothing. */
  | 'independent';

/** What this repository currently says about itself. */
export function projectSettings(root: string): Promise<ProjectSettings> {
  return picus('picus_project_settings', { root });
}

/**
 * Write the project-wide settings.
 *
 * The whole set is replaced rather than patched field by field: these come from
 * one form the user pressed Save on, and a partial write would leave the file
 * describing a state nobody chose. Everything the form does not cover — the
 * folder declarations, the aliases, the naming scheme — is untouched.
 */
export function setProjectSettings(
  root: string,
  settings: ProjectSettings,
): Promise<ConfirmedProject> {
  return picus('picus_set_project_settings', { root, settings });
}

/**
 * Declare — or forget — the engine of **one file**.
 *
 * The leaf of the same chain {@link confirmProject} and {@link setFolderAlias}
 * sit on, and the one that answers for an untidy repository: a directory holding
 * `4_12_ORA.sql` beside `4_12_POS.sql` can say nothing true about either, and
 * neither a folder declaration nor a name rule fits a one-off.
 *
 * Two-valued rather than three, unlike {@link ProjectEdit}: this names one file
 * and one field, so there is no "leave it alone" to encode — not calling it is
 * what leaves it alone. Passing `null` **clears** the declaration and the file
 * goes back to inheriting its folder.
 *
 * Answers with the same shape a confirmation does, because the same thing
 * happened: `.arbor/picus/project.toml` was written and the tree re-resolved.
 */
export function setFileEngine(
  root: string,
  path: string,
  dialect: FolderEngine | null,
): Promise<ConfirmedProject> {
  return picus('picus_set_file_engine', { root, path, dialect });
}

/**
 * Take a folder or a script out of the project — or put it back.
 *
 * **One verb for both**, because it is one decision and the user is pointing at
 * one row: `path` names whichever it is, and a folder path and a file path
 * cannot collide in a tree built from real directories.
 *
 * **This is not the `ignored` role.** An ignored folder is still read, still
 * indexed and still checked — it simply is not an installation folder and
 * nothing is generated into it, which is worth knowing about a folder of old
 * migrations. An excluded one is treated as though it were not in the
 * repository: not parsed, not indexed, no coverage column, no findings, never a
 * destination. The two cannot be merged, because `ignored` is also the fallback
 * for a folder nobody has classified.
 *
 * Two-valued, like {@link setFileEngine} and for the same reason: this names one
 * row and one field, so not calling it is what leaves it alone. `false` on a
 * **file** is not a no-op — it rescues that one script from an excluded folder.
 *
 * Answers with the same shape a confirmation does, because the same thing
 * happened: `.arbor/picus/project.toml` was written and the tree re-resolved.
 */
export function setExcluded(
  root: string,
  path: string,
  excluded: boolean,
): Promise<ConfirmedProject> {
  return picus('picus_set_excluded', { root, path, excluded });
}

// ── Named sets of destinations ───────────────────────────────────────────────

/**
 * One entry of a set, **resolved against the repository as it is now**.
 *
 * Everything `dmlStore.addTarget` needs plus the rules, so applying a set is a
 * loop over these and nothing else. See {@link destinationSets} for why the
 * resolution happens on the backend.
 */
export interface ResolvedDestination {
  /** The folder as stored — how a failed entry is still named to the user. */
  folder: string;
  /** Project-relative file path. Empty when the entry could not be resolved. */
  file: string;
  /** The file does not exist yet — the ordinary case for a new update script. */
  createsFile: boolean;
  dialect?: FolderEngine;
  role: FolderRole;
  /** The folder's product, for the version row. Absent when none is declared. */
  product?: string;
  wrap: 'block' | 'plain';
  versionGuard: boolean;
  skipIfPresent: boolean;
  requireObject: boolean;
  transactional: boolean;
  /** What the naming scheme says this file moves between, when it could say. */
  fromVersion?: string;
  toVersion?: string;
  /** This entry names one fixed file instead of following the folder's naming
   *  scheme, so it keeps writing into that file next release. Not a failure —
   *  for a folder the scheme cannot read it is the only thing that works — but
   *  the set's "still works next release" promise does not cover it. */
  pinned: boolean;
  /** Why this entry cannot be used. Per entry, so one dead folder costs one
   *  destination rather than the whole set. */
  problem?: string;
}

export interface ResolvedSet {
  name: string;
  destinations: ResolvedDestination[];
}

/**
 * The named sets of destinations this repository declares, resolved.
 *
 * Resolved on the backend rather than pasted on the frontend, because half the
 * paths in a set are different every release: an entry stores a **folder**, and
 * turning that into "this release's update file, moving 4.12 to 4.13" needs the
 * repository's naming scheme, which lives there. Doing it here would be a second
 * implementation of a rule that must not drift.
 */
export function destinationSets(root: string): Promise<ResolvedSet[]> {
  return picus('picus_destination_sets', { root });
}

/** What a set looks like on the way in — folders and rules, never resolved paths. */
export interface DestinationSetInput {
  name: string;
  entries: {
    folder: string;
    /** The file as it stands. Whether it can be dropped in favour of "the next
     *  update file, whatever the scheme calls it" is decided on the backend,
     *  which can read the folder — send it always. */
    file?: string;
    wrap?: 'block' | 'plain';
    versionGuard?: boolean;
    /** The guard's bounds as they stand. Kept only for an entry whose file the
     *  naming scheme cannot re-derive — decided on the backend, like `file`. */
    fromVersion?: string;
    toVersion?: string;
    skipIfPresent?: boolean;
    requireObject?: boolean;
    transactional?: boolean;
  }[];
}

/** Save a set under its name, replacing one of the same name. */
export function saveDestinationSet(
  root: string,
  set: DestinationSetInput,
): Promise<ConfirmedProject> {
  return picus('picus_save_destination_set', { root, set });
}

export function deleteDestinationSet(root: string, name: string): Promise<ConfirmedProject> {
  return picus('picus_delete_destination_set', { root, name });
}

/**
 * Say which installed product a folder's scripts belong to — or forget it.
 *
 * The counterpart of `products` in {@link ProjectSettings}: that says *what a
 * product is* (which row of the version table is its), this says *where its
 * scripts live*. Together they mean a generated block written into `PORTALE/…`
 * stamps the portal's row without anyone retyping the predicate per generation.
 *
 * Folders only, and it inherits: naming the product once at the top of `PORTALE/`
 * answers for every version folder underneath, including next month's.
 *
 * Two-valued, like {@link setFileEngine} and for the same reason — this names one
 * row and one field, so not calling it is what leaves it alone. `null` clears the
 * declaration and the folder goes back to inheriting.
 */
export function setFolderProduct(
  root: string,
  path: string,
  product: string | null,
): Promise<ConfirmedProject> {
  return picus('picus_set_folder_product', { root, path, product });
}

/**
 * Declare — or forget — what a **name** means in this repository.
 *
 * The other half of classification, and the half that scales: an edit answers for
 * one path, this answers for every folder called `POS` **including the ones that
 * do not exist yet**. A repository shipping a folder set per delivered version
 * cannot be described any other way without re-describing it every release.
 *
 * Every field is **replaced**, not merged — an alias has exactly these three, so
 * "set it to this" needs none of the three-valued machinery {@link ProjectEdit}
 * needs. Which is also the trap: `appliesTo` is not optional here on purpose,
 * because omitting it does not mean "keep what the alias already said", it means
 * "folders only". Every caller passes the scope it wants, including the ones that
 * are only editing the engine.
 *
 * Passing neither an engine nor a role removes the alias.
 */
export function setFolderAlias(
  root: string,
  name: string,
  engine: FolderEngine | null,
  role: FolderRole | null,
  appliesTo: AliasScope,
): Promise<ConfirmedProject> {
  return picus('picus_set_folder_alias', {
    root,
    name,
    engine,
    role,
    applies_to: appliesTo,
  });
}

/**
 * Every folder an alias of this name would apply to, in tree order.
 *
 * Asked **before** the alias is offered, so the offer can say "and the other ten
 * folders called POS" with a number that is true. The matching rule is not
 * reimplemented here on purpose: `POS` matching `01_POS` but not `POSIZIONI` is
 * load-bearing, and a second copy of a load-bearing rule is a copy that drifts.
 */
export function foldersNamed(root: string, name: string): Promise<string[]> {
  return picus('picus_folders_named', { root, name });
}

/**
 * Every **file** whose name an alias would apply to.
 *
 * The twin of {@link foldersNamed} and asked for the same reason: the offer to
 * turn one classification into a repository-wide rule is only safe to accept
 * because the number beside it is true. Matching the stem rather than the whole
 * name, and whole words rather than substrings, are rules that live in
 * `picus-project` — a copy of them in the interface would be a copy that drifts.
 */
export function filesNamed(root: string, name: string): Promise<string[]> {
  return picus('picus_files_named', { root, name });
}
