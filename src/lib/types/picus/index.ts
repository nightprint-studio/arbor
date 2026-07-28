/**
 * Picus (SQL studio) domain types — the shapes the UI renders.
 *
 * Picus has two halves that meet in the DML generator:
 *  • a **database client** (multiple simultaneous Oracle / PostgreSQL sessions,
 *    schema browsing, query editor, data grid), and
 *  • a **maintainer of SQL scripts on disk**, where the same logical change has
 *    to be written into an Oracle folder AND a PostgreSQL folder, in different
 *    syntactic forms.
 *
 * The single structural invariant of the whole product: **the dialect is a
 * property of the FOLDER, never a global "current dialect"**. Every type that
 * can produce or analyse SQL carries its `Dialect` explicitly.
 *
 * These are the `picus-be` wire types: the Rust side serialises camelCase field
 * for field, so nothing translates between the backend and what is rendered.
 * The request/response envelopes around them live in `ipc/picus/{db,scripts}.ts`.
 */

// ── Dialects ─────────────────────────────────────────────────────────────────

export type Dialect = 'oracle' | 'postgres';

export interface DialectInfo {
  id: Dialect;
  /** Short label used on chips ("Oracle", "PostgreSQL"). */
  short: string;
  /** Full product label used in tooltips / details ("Oracle 19c"). */
  label: string;
  /** Theme token holding the dialect's identity colour. */
  colorVar: string;
}

export const DIALECTS: Record<Dialect, DialectInfo> = {
  oracle: {
    id: 'oracle',
    short: 'Oracle',
    label: 'Oracle 19c',
    // The workspace palette is the app-wide "identity colour" ramp; reusing it
    // keeps dialect colours inside the theme instead of hardcoding hex.
    colorVar: '--ws-color-1',
  },
  postgres: {
    id: 'postgres',
    short: 'PostgreSQL',
    label: 'PostgreSQL 16',
    colorVar: '--ws-color-0',
  },
};

/**
 * An engine Picus **recognises and does not support**.
 *
 * The third state, and it is not the same as "no engine". A repository whose
 * `MSQ` folders are SQL Server was previously stuck being asked, forever, a
 * question with no available answer — and a tool that keeps asking something you
 * have already answered is one people stop reading.
 *
 * A folder in this state is named on screen, left out of every lane, comparison
 * and coverage column, and — the part that matters most — **never parsed**: a
 * permissive Oracle/PostgreSQL grammar does not fail on T-SQL, it produces
 * plausible-looking nonsense.
 */
export type ForeignEngine = 'sqlserver' | 'db2' | 'mysql' | 'mariadb' | 'sqlite';

/** How each unsupported engine is spelled, matching `picus-types`' `label()`. */
export const FOREIGN_ENGINES: Record<ForeignEngine, string> = {
  sqlserver: 'SQL Server',
  db2: 'DB2',
  mysql: 'MySQL',
  mariadb: 'MariaDB',
  sqlite: 'SQLite',
};

/** Every unsupported engine, in the order the pickers list them. */
export const FOREIGN_ENGINE_CHOICES: ForeignEngine[] = [
  'sqlserver', 'db2', 'mysql', 'mariadb', 'sqlite',
];

/**
 * **Portable SQL**: valid on every dialect Picus supports.
 *
 * The folders of plain `INSERT` / `UPDATE` / `DELETE` that are meant to run on
 * Oracle *and* on PostgreSQL. Before this existed they had to be declared as one
 * or the other, which was a lie either way — and the lie had consequences: the
 * dialect they were not got reported as missing everything they contained.
 *
 * It is **never inferred**. No folder name produces it. A promise that these
 * scripts run on both engines is something a person makes, not something a name
 * implies, so it only ever arrives because somebody declared it.
 */
export const GENERIC_ENGINE = 'generic';

/**
 * What a folder can be declared as — all four answers in one value, because a
 * folder has one engine.
 *
 * It crosses the wire as a single string in a single key, which is why
 * `dialect = "oracle"`, `"generic"` and `"sqlserver"` are the same field in the
 * project file.
 */
export type FolderEngine = Dialect | typeof GENERIC_ENGINE | ForeignEngine;

/**
 * What a generation may be emitted as: one dialect, or portable.
 *
 * Deliberately narrower than `FolderEngine` — there is no member for an engine
 * Picus does not support, so a destination in such a folder is unrepresentable.
 * The same distinction the backend's `DialectScope` makes, for the same reason.
 */
export type TargetScope = Dialect | typeof GENERIC_ENGINE;

/** Is this engine one Picus reads and generates as a single dialect? */
export function isDialect(engine: FolderEngine | null | undefined): engine is Dialect {
  return engine === 'oracle' || engine === 'postgres';
}

/** Portable SQL — valid on both dialects, and emitted as their intersection. */
export function isGenericEngine(engine: FolderEngine | null | undefined): boolean {
  return engine === GENERIC_ENGINE;
}

/** An engine Picus recognises and does not read. */
export function isForeignEngine(
  engine: FolderEngine | null | undefined,
): engine is ForeignEngine {
  return !!engine && engine in FOREIGN_ENGINES;
}

/** How to spell any engine — dialect, portable, or unsupported. */
export function engineLabel(engine: FolderEngine): string {
  if (isDialect(engine)) return DIALECTS[engine].short;
  if (isGenericEngine(engine)) return 'Portable SQL';
  return FOREIGN_ENGINES[engine as ForeignEngine];
}

/**
 * Every dialect a folder with this engine answers for.
 *
 * Two for portable, one for a dialect, none for an unsupported engine — the
 * question every lane, coverage column and cross-dialect comparison asks, and the
 * reason a portable folder is the first thing in the model to be in two lanes.
 */
export function enginesCovered(engine: FolderEngine | null | undefined): Dialect[] {
  if (isDialect(engine)) return [engine];
  if (isGenericEngine(engine)) return ['oracle', 'postgres'];
  return [];
}

/**
 * A folder **name** that means something in this repository.
 *
 * The built-in vocabulary is a global heuristic and can only hold names that mean
 * one thing everywhere — `ORA` is Oracle in every repository, `POS` is not
 * PostgreSQL in every repository. An alias is the local fact its owner knows,
 * and it answers for *every* folder of that name, including the ones the next
 * release will add. That is what a per-path declaration can never do.
 *
 * `engine` and `role` are wire strings rather than the unions, deliberately: the
 * backend keeps them as strings so a hand-edited typo degrades to "this alias
 * does nothing" and is reported, instead of failing the whole file's parse. The
 * interface renders whatever it is given.
 */
export interface FolderAlias {
  /** The name as the repository spells it. Matched whole-word, case-insensitively. */
  name: string;
  /** `oracle` · `postgres` · an unsupported engine · absent. */
  engine?: string | null;
  /** A `FolderRole` wire word, or absent. */
  role?: string | null;
}

// ── Connections ──────────────────────────────────────────────────────────────

export type ConnectionState = 'connected' | 'read-only' | 'disconnected' | 'connecting';

/**
 * One live database session. `colorIdx` indexes the shared workspace palette
 * (`--ws-color-N`) — the same identification mechanism as Corvus workspaces:
 * the colour shows on the sidebar row, on every tab bound to this connection,
 * and in the status bar.
 */
export interface Connection {
  id: string;
  name: string;
  /** Human-readable role: "development", "staging", "production". */
  alias: string;
  dialect: Dialect;
  /** Server-side schema / search_path the session is pinned to. */
  schema: string;
  host: string;
  state: ConnectionState;
  /** The application version stamped in the version table, when readable. */
  dbVersion: string;
  colorIdx: number;
  /**
   * Refuse every non-read statement in the BACKEND, not just in the UI. Mirrored
   * here so the UI can grey the write affordances too.
   */
  readOnly: boolean;
}

// ── Schema ───────────────────────────────────────────────────────────────────

export interface Column {
  name: string;
  /** Native type as the server reports it (`VARCHAR2(30)`, `numeric(5,2)`). */
  type: string;
  primaryKey?: boolean;
  notNull?: boolean;
  /** Server-side default expression, when the column has one. */
  defaultValue?: string;
}

/** A referential constraint, with what happens to the child rows on delete. */
export interface ForeignKey {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onDelete?: 'CASCADE' | 'SET NULL' | 'NO ACTION' | 'RESTRICT';
}

export interface IndexInfo {
  name: string;
  columns: string[];
  unique: boolean;
  /** Server-side index kind, as reported (`BTREE`, `BITMAP`, `FUNCTION-BASED`). */
  kind?: string;
  /** True for the index backing the primary key — it is not a separate object
   *  the user created, and deleting it is not an option. */
  primaryKey?: boolean;
}

/**
 * A table or a view. Views carry their defining query instead of constraints —
 * everything else about them (columns, types) reads the same, which is why they
 * share a shape rather than a hierarchy.
 */
export interface TableInfo {
  name: string;
  kind: 'table' | 'view';
  columns: Column[];
  primaryKeyName?: string;
  foreignKeys?: ForeignKey[];
  indexes?: IndexInfo[];
  /** Views only: the SELECT the view is defined as. */
  definition?: string;
  /** Approximate row count, when the server can give one cheaply. */
  estimatedRows?: number;
}

export interface SequenceInfo {
  name: string;
  lastValue: number;
  incrementBy: number;
  minValue?: number;
  maxValue?: number;
  cycle: boolean;
  cacheSize?: number;
}

export interface TriggerInfo {
  name: string;
  /** Table the trigger is attached to. */
  table: string;
  timing: 'BEFORE' | 'AFTER' | 'INSTEAD OF';
  /** `INSERT`, `UPDATE`, `DELETE` — a trigger can answer to several. */
  events: string[];
  enabled: boolean;
  /** Row-level vs statement-level. */
  forEachRow: boolean;
}

/** The schema of one connection, as far as it has been read. */
export interface SchemaSnapshot {
  tables: TableInfo[];
  views: TableInfo[];
  sequences: SequenceInfo[];
  triggers: TriggerInfo[];
}

/** The groups the schema tree offers, in display order. */
export type SchemaGroup = 'tables' | 'views' | 'sequences' | 'triggers';

export const SCHEMA_GROUP_LABELS: Record<SchemaGroup, string> = {
  tables: 'Tables',
  views: 'Views',
  sequences: 'Sequences',
  triggers: 'Triggers',
};

// ── Scripts on disk ──────────────────────────────────────────────────────────

/** What a folder of scripts is FOR — drives the generator's target presets. */
export type FolderRole = 'init' | 'update' | 'routines' | 'data' | 'ignored';

export const FOLDER_ROLE_LABELS: Record<FolderRole, string> = {
  init: 'initialisation',
  update: 'update',
  routines: 'routines',
  data: 'data',
  ignored: 'ignored',
};

/** Short form for chips where the full role label doesn't fit. */
export const FOLDER_ROLE_SHORT: Record<FolderRole, string> = {
  init: 'init',
  update: 'upd',
  routines: 'proc',
  data: 'data',
  ignored: '—',
};

export type LineEnding = 'CRLF' | 'LF';

/** How a file's encoding was decided — surfaced so the guess is never silent. */
export type EncodingSource =
  /** A byte-order mark declared it. */
  | 'bom'
  /** Valid UTF-8 with at least one multibyte sequence. */
  | 'utf8'
  /** Pure ASCII: ambiguous, inherited from the folder's dominant encoding. */
  | 'inherited'
  /** Single-byte heuristic. */
  | 'heuristic'
  /** Pinned by the user. */
  | 'forced';

export interface ScriptFile {
  /** Path relative to the project root, POSIX separators. */
  path: string;
  name: string;
  size: number;
  encoding: string;
  encodingSource: EncodingSource;
  eol: LineEnding;
  /**
   * Encoding the project expects for this folder. When it differs from
   * `encoding` the file was probably rewritten by an external editor — an
   * `ENC001` finding.
   */
  expectedEncoding: string;
  /** Working-copy marker shown on the tree row. */
  status?: 'modified' | 'new' | 'error';
}

/**
 * One directory of the repository, as it actually is on disk.
 *
 * There is no "branch" level and no invented grouping: the tree Picus shows is
 * the tree the user has. A repository whose layout is
 * `AGGIORNAMENTO/<version>/ORA` puts the dialect five levels down and the role
 * at the top; another puts them the other way round. Both are described by the
 * same rule — **any directory may declare a dialect and/or a role, and every
 * directory below it inherits that declaration until one overrides it**.
 *
 * Hence the two pairs of fields, which are NOT redundant:
 *  • `engine` / `role` are what this folder **declares** — `null` means "says
 *    nothing, ask my ancestors". They are what a correction writes.
 *  • `effectiveEngine` / `effectiveRole` are the answer **after inheritance** —
 *    what the folder actually is. `effectiveEngine: null` is a real answer
 *    (nobody up the chain said), and `effectiveRole` falls back to `ignored`.
 *
 * Showing only the effective pair would leave the user with no way to tell where
 * to go to change it; showing only the declared pair would leave most rows blank.
 *
 * ## One engine field, four answers
 *
 * A folder has one engine, so it has one field, and it is one of four things: a
 * dialect Picus reads; **portable** SQL valid on both; an engine it only
 * recognises; or nothing yet. Read it through the helpers below rather than the
 * field, because the useful questions are not "which engine" but "what do I
 * generate" ({@link dialectOf}, `null` for portable) and "does this count for
 * that engine" ({@link folderCovers}, true of **both** for portable).
 */
export interface FolderNode {
  /** Project-relative path, POSIX separators. **The identity** of the folder. */
  path: string;
  /** Last path segment — what the row shows. */
  name: string;
  /** Engine DECLARED on this folder; `null` means inherit from an ancestor. */
  engine: FolderEngine | null;
  /** Role DECLARED on this folder; `null` means inherited from an ancestor. */
  role: FolderRole | null;
  /**
   * Engine after inheritance. `null` is a real answer and means exactly one
   * thing: **nobody has said**. It is not what a portable folder gets and not
   * what an unsupported one gets — both of those are answers.
   */
  effectiveEngine: FolderEngine | null;
  /** Role after inheritance. `ignored` when nobody said. */
  effectiveRole: FolderRole;
  children: FolderNode[];
  files: ScriptFile[];
}

/** What this folder **declares**; `null` = it says nothing and inherits. */
export function declaredEngine(node: FolderNode): FolderEngine | null {
  return node.engine;
}

/** What applies here after inheritance. */
export function folderEngine(node: FolderNode): FolderEngine | null {
  return node.effectiveEngine;
}

/**
 * The **single** dialect this folder's scripts are generated as, if it has one.
 *
 * `null` for a portable folder as well as for an unclassified one, which is
 * correct in both cases: a portable folder has no single dialect, it has two.
 * Callers that meant "which side of the comparison is this" want
 * {@link folderCovers}.
 */
export function dialectOf(node: FolderNode): Dialect | null {
  const engine = node.effectiveEngine;
  return isDialect(engine) ? engine : null;
}

/** Does content in this folder count as present for `dialect`? */
export function folderCovers(node: FolderNode, dialect: Dialect): boolean {
  return enginesCovered(node.effectiveEngine).includes(dialect);
}

/** Portable SQL: written to run on every dialect Picus supports. */
export function isGeneric(node: FolderNode): boolean {
  return isGenericEngine(node.effectiveEngine);
}

/** An engine Picus recognises and does not read — an answer, never a question. */
export function engineIsUnsupported(node: FolderNode): boolean {
  return isForeignEngine(node.effectiveEngine);
}

/**
 * Nobody knows what engine this folder is — the state that is **asked about**.
 *
 * Explicitly not true of a portable or an unsupported engine: those are answers,
 * and asking again would be asking a question the user has already answered.
 */
export function engineIsUnknown(node: FolderNode): boolean {
  return node.effectiveEngine === null;
}

/** Can a generation be written into this folder? */
export function folderAcceptsGeneration(node: FolderNode): boolean {
  return isDialect(node.effectiveEngine) || isGeneric(node);
}

/**
 * Where the installed version is recorded, and what to do with it.
 *
 * Every project stamps its version somewhere, but not in the same shape: the
 * table name, the column, and whether there is a date column at all all differ.
 * Hardcoding `VERSIONE_DB.VERSIONE` would make the version guard — the single
 * most valuable rule Picus has — work on exactly one project.
 */
export interface VersionTableConfig {
  /** Table holding the installed version. Empty disables version guards. */
  table: string;
  /** Column holding the version string. */
  versionColumn: string;
  /**
   * Column stamped with the moment of the upgrade. `null` when the project
   * doesn't track one — the emitter then leaves it out of the UPDATE entirely
   * rather than inventing a column.
   */
  dateColumn: string | null;
  /**
   * Extra predicate for projects whose version table holds one row per module
   * (`WHERE MODULO = 'CORE'`). Empty means "the table holds a single row".
   */
  filter: string;
}

export const DEFAULT_VERSION_TABLE: VersionTableConfig = {
  table: 'VERSIONE_DB',
  versionColumn: 'VERSIONE',
  dateColumn: 'DATA_AGG',
  filter: '',
};

export interface Project {
  name: string;
  root: string;
  /** The repository's real directory hierarchy, from the root's own children. */
  tree: FolderNode[];
}

// ── Inventory ────────────────────────────────────────────────────────────────

export type ObjectKind =
  | 'table' | 'view' | 'sequence' | 'package' | 'procedure' | 'function' | 'trigger';

/**
 * One indexed database object and how the repository covers it.
 *
 * `coverage` is keyed by **folder path** — the same identity `FolderNode.path`
 * carries — and holds the number of statements in that folder which touch the
 * object. `0` (or an absent key) is the interesting value: a place that stays
 * silent about something another place says, which is what the `CONS001` family
 * reports.
 *
 * A real repository has hundreds of folders, so nothing renders this map
 * folder-by-folder. `utils/picus/coverage.ts` folds it into the axes the rules
 * actually compare — engine × role — and keeps the per-folder detail for the
 * one object being looked at.
 */
export interface InventoryObject {
  name: string;
  kind: ObjectKind;
  coverage: Record<string, number>;
}

// ── Consistency ──────────────────────────────────────────────────────────────

export type Severity = 'blocking' | 'review';

/** Stable rule identifiers (§4.3). Kept as a union so a typo can't invent one. */
export type RuleId =
  | 'CONS001' | 'CONS002' | 'CONS003' | 'CONS004'
  | 'DIA001'
  | 'VER001' | 'VER002' | 'VER003'
  | 'DUP001' | 'DUP002'
  | 'ENC001' | 'ENC002'
  | 'DML001' | 'DML002';

export interface Finding {
  id: string;
  rule: RuleId;
  severity: Severity;
  title: string;
  /** What goes wrong in practice if this is left alone — never a rule restatement. */
  consequence: string;
  /** Project-relative file the finding anchors to. */
  file: string;
  /** 1-based line, when the rule can point at one. */
  line?: number;
  /** Extra locations for rules that pair two places (e.g. a duplicate). */
  alsoAt?: string;
  /** Label of the corrective action, when the rule can propose a patch. */
  fixLabel?: string;
  /**
   * Declared suppression: the reason from a `-- picus: ignore DML001 — …`
   * comment. Present means the finding is silenced but still visible.
   */
  suppressedBecause?: string;
}

// ── DML generator ────────────────────────────────────────────────────────────

export type DmlSource = 'form' | 'paste' | 'csv';
export type DmlOperation = 'insert' | 'upsert' | 'update' | 'delete';

export const DML_OPERATION_LABELS: Record<DmlOperation, string> = {
  insert: 'INSERT',
  upsert: 'INSERT if missing',
  update: 'UPDATE',
  delete: 'DELETE',
};

/** A row of values keyed by column name. Empty string means "not supplied". */
export type DmlRow = Record<string, string>;

/** How a target wraps the statements it receives. */
export type TargetWrap = 'plain' | 'block';

export interface VersionGuard {
  from: string;
  to: string;
}

export interface TargetGuards {
  /** Run only when the database is at `from`, then carry it to `to`. */
  version: VersionGuard | null;
  /** Skip rows already present, matched on the comparison key. */
  skipIfPresent: boolean;
  /** Bail out when the table doesn't exist (`USER_TABLES` / `to_regclass`). */
  requireObject: boolean;
  /** Savepoint + rollback on error. */
  transactional: boolean;
}

/**
 * One file the generation is written into. Every target carries its own dialect
 * and its own rules: the same logical change becomes a bare INSERT in the Oracle
 * init script and a guarded PL/SQL block in the Oracle update script.
 *
 * A target names **a file and a dialect** — nothing above it. The dialect and the
 * role are copied from the destination folder's *effective* values at the moment
 * the destination is added, and stay editable afterwards; a folder with no engine
 * cannot become a destination at all, because there is no form to write in.
 */
export interface Target {
  id: string;
  /** Project-relative path of the destination file. */
  file: string;
  /**
   * One dialect, or `generic` for a portable destination.
   *
   * A portable target accepts only what **both** engines accept: plain
   * statements, no procedural block, no upsert, and therefore no version guard.
   * The backend refuses the rest with the reason rather than emitting something
   * that runs on one engine — the payoff being that a portable destination writes
   * one file where two used to be needed.
   */
  dialect: TargetScope;
  role: FolderRole;
  enabled: boolean;
  wrap: TargetWrap;
  guards: TargetGuards;
}

// ── Query results ────────────────────────────────────────────────────────────

/** A cell value. `null` is a real SQL NULL and renders differently from ''. */
export type CellValue = string | number | null;

/**
 * A result set is NOT a value here.
 *
 * A read opens a held cursor on the server and the UI holds a window onto it, so
 * "the rows" is a live handle with a length, a loaded extent and a lifetime —
 * `stores/picus/result` owns that shape, and the wire types for its calls live in
 * `ipc/picus/db`. A plain rows-and-count record used to stand here, and it could
 * only ever describe a result small enough to have been fetched whole.
 */
export interface QueryLogEntry {
  time: string;
  text: string;
  level: 'info' | 'error';
}

// ── Editor tabs ──────────────────────────────────────────────────────────────

export type TabKind = 'generate' | 'query' | 'table' | 'file' | 'inventory';

export interface PicusTab {
  id: string;
  kind: TabKind;
  title: string;
  /** Connection this tab executes against — tabs bound to a database only. */
  connectionId?: string;
  /** Object name, for `table` tabs (a table, view, sequence or trigger). */
  table?: string;
  /**
   * Which kind of schema object a `table` tab is showing. They share one tab
   * kind because they share the frame — name, connection, sub-views — and
   * differ only in what those sub-views contain.
   */
  objectKind?: Extract<ObjectKind, 'table' | 'view' | 'sequence' | 'trigger'>;
  /** Project-relative path, for `file` tabs. */
  file?: string;
  /**
   * Engine of the file or connection, shown as a chip on the tab.
   *
   * A `FolderEngine` and not a `Dialect`, because a file in a portable folder has
   * no single dialect — it has two — and the chip says so rather than picking one.
   */
  dialect?: FolderEngine;
  /** Unsaved changes marker. */
  dirty?: boolean;
  /**
   * 1-based line the view should reveal — set when a finding, or any other
   * located thing, opens the file at a place rather than at the top.
   */
  revealLine?: number;
  /**
   * Bumped on every reveal request. Two consecutive jumps to the same line are
   * two jumps, and without this the second would look like it was ignored.
   */
  revealNonce?: number;
}
