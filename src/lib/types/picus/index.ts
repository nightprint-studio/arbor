/**
 * Picus (SQL studio) domain types — the shapes the UI renders.
 *
 * Picus has two halves that meet in the DML generator:
 *  • a **database client** (multiple simultaneous Oracle / PostgreSQL sessions,
 *    schema browsing, query editor, data grid), and
 *  • a **maintainer of SQL scripts on disk**, where the same logical change has
 *    to be written into an Oracle branch AND a PostgreSQL branch, in different
 *    syntactic forms.
 *
 * The single structural invariant of the whole product: **the dialect is a
 * property of the FOLDER, never a global "current dialect"**. Every type that
 * can produce or analyse SQL carries its `Dialect` explicitly.
 *
 * These mirror the eventual `picus-be` wire types; until that backend exists the
 * stores fill them from `components/picus/mock` (see the note there).
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

export interface ScriptFolder {
  id: string;
  label: string;
  role: FolderRole;
  /** Path relative to the project root. */
  path: string;
  files: ScriptFile[];
}

/** A per-dialect branch of the script repository. */
export interface Branch {
  id: string;
  label: string;
  dialect: Dialect;
  folders: ScriptFolder[];
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
  branches: Branch[];
}

// ── Inventory ────────────────────────────────────────────────────────────────

export type ObjectKind =
  | 'table' | 'view' | 'sequence' | 'package' | 'procedure' | 'function' | 'trigger';

/**
 * One indexed database object and how each branch/folder covers it. `coverage`
 * is keyed by `"<branchId>/<folderId>"` and holds the number of statements that
 * touch the object there — `0` is the interesting value (a gap between
 * branches), which is what `CONS001` reports.
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
  | 'CONS001' | 'CONS002' | 'CONS003'
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
  branchId: string;
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
 */
export interface Target {
  id: string;
  /** Project-relative path of the destination file. */
  file: string;
  dialect: Dialect;
  role: FolderRole;
  branchId: string;
  enabled: boolean;
  wrap: TargetWrap;
  guards: TargetGuards;
}

// ── Query results ────────────────────────────────────────────────────────────

/** A cell value. `null` is a real SQL NULL and renders differently from ''. */
export type CellValue = string | number | null;

export interface QueryResult {
  columns: Column[];
  rows: CellValue[][];
  /** Server-side elapsed time in ms. */
  elapsedMs: number;
  /** Rows actually fetched (may be capped by the row limit). */
  rowCount: number;
  /** True when the row limit truncated the result. */
  truncated: boolean;
}

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
  /** Dialect of the file/connection, shown as a chip on the tab. */
  dialect?: Dialect;
  /** Unsaved changes marker. */
  dirty?: boolean;
}
