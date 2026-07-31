/**
 * Picus database IPC — connections, schema, rows, statements.
 *
 * Everything here goes through the generic `picus(...)` rpc bridge to `picus-be`,
 * except the three password calls, which are **Tauri commands straight to the
 * shell**. That asymmetry is deliberate: the password goes from the form to
 * Arbor's keychain without ever entering the backend process. `picus-be` asks the
 * shell for it over the reverse channel at the moment it opens a session.
 *
 * The schema/query payloads are the same shapes as `$lib/types/picus` — the Rust
 * side serialises camelCase precisely so there is no translation layer here.
 */

import { invoke } from '@tauri-apps/api/core';

import type {
  CellValue,
  Column,
  Predicate,
  Dialect,
  DmlOperation,
  DmlRow,
  SchemaGroup,
  SchemaSnapshot,
  TableInfo,
  TriggerDetail,
  Target,
  VersionTableConfig,
} from '$lib/types/picus';
import { picus } from '../rpc';

// ── Provider descriptors (the data-driven per-engine UI) ─────────────────────

/** How a connection field is rendered and validated. */
export type FieldType = 'text' | 'number' | 'secret' | 'select' | 'toggle';

export interface SelectOption {
  value: string;
  label: string;
}

/** One field of the create-connection form, as the backend declares it. */
export interface ConnectionField {
  id: string;
  label: string;
  /** Discriminant of the flattened `FieldKind`. */
  type: FieldType;
  /** `number` only. */
  min?: number;
  max?: number;
  /** `select` only. */
  options?: SelectOption[];
  default?: string | null;
  placeholder?: string | null;
  required: boolean;
  help?: string | null;
}

/** What an engine has. The UI reads this instead of asking "is this Oracle?". */
export interface EngineCapabilities {
  /** False for an engine Picus supports on the script side only — Oracle today. */
  connect: boolean;
  sequences: boolean;
  materializedViews: boolean;
  packages: boolean;
  insteadOfTriggers: boolean;
  bitmapIndexes: boolean;
  expressionIndexes: boolean;
  cancelQuery: boolean;
  estimatedRows: boolean;
  schemas: boolean;
  /** The server can say what every session is doing — drives the session monitor. */
  sessionActivity: boolean;
  /** A statement's plan can be read without running it. */
  explain: boolean;
  /** A statement can be prepared without running it — drives the editor's live
   *  validation against the server. */
  validate: boolean;
  /** Statements can carry bound values rather than interpolated ones. */
  bindParameters: boolean;
  /** The catalogue can be walked into a dependency graph. */
  dependencyGraph: boolean;
  transactions: TxCapability;
}

/**
 * What an explicit transaction actually covers.
 *
 * `transactionalDdl` is the field that matters and the reason this is not a
 * boolean: PostgreSQL undoes a `CREATE TABLE` on rollback and Oracle cannot, so
 * "everything in one transaction" is a promise only one of the two engines can
 * keep. The interface states that before a run rather than explaining it after.
 */
export interface TxCapability {
  supported: boolean;
  transactionalDdl: boolean;
  savepoints: boolean;
}

/** The dialect differences the generator needs, as data. */
export interface EmissionTraits {
  blockOpen: string;
  blockClose: string;
  statementTerminator: string;
  nowFunction: string;
  upsertForm: string;
  objectExistsCheck: string;
  identifierCase: 'upper' | 'lower';
  /** True when DDL commits implicitly, so a "roll back on error" rule cannot hold. */
  ddlCommitsImplicitly: boolean;
}

export interface DbProviderDescriptor {
  kind: Dialect;
  label: string;
  shortLabel: string;
  /** Theme token, never a hex literal. */
  colorVar: string;
  defaultPort: number;
  fields: ConnectionField[];
  capabilities: EngineCapabilities;
  emission: EmissionTraits;
  schemaGroups: SchemaGroup[];
}

/** Every engine Picus knows, connectable or not, in display order. */
export function listProviders(): Promise<DbProviderDescriptor[]> {
  return picus('picus_providers', {});
}

// ── Connections ──────────────────────────────────────────────────────────────

/** A connection as configured. Never holds a password — by construction. */
export interface ConnectionSpec {
  id: string;
  name: string;
  alias: string;
  engine: Dialect;
  host: string;
  port: number;
  database: string;
  user: string;
  schema: string;
  colorIdx: number;
  readOnly: boolean;
  tls: boolean;
  /**
   * The repository of install scripts this database is built from — an absolute
   * path in the platform's own form, absent when none is attached yet.
   *
   * A first-class field rather than an entry in `params`, because the backend has
   * one and reads it: those scripts install *this* database, which is the spine of
   * the product rather than a convenience. Anything written into `params` would be
   * round-tripped faithfully and still never reach it.
   */
  scriptRoot?: string;
  /** Engine-specific extras a descriptor declared but the spec has no field for. */
  params: Record<string, string>;
}

export type ConnectionState = 'connected' | 'read-only' | 'disconnected' | 'connecting';

/** A configured connection plus its live state — the sidebar row. */
export interface ConnectionRow extends ConnectionSpec {
  state: ConnectionState;
  serverVersion: string;
  /** Whether a password is stored. Never the password itself. */
  hasSecret: boolean;
}

export interface ConnectionStatus {
  id: string;
  state: ConnectionState;
  serverVersion: string;
  dbVersion: string;
  message: string;
}

export function listConnections(): Promise<ConnectionRow[]> {
  return picus('picus_list_connections', {});
}

export function saveConnection(connection: ConnectionSpec): Promise<void> {
  return picus('picus_save_connection', { connection });
}

/** Forget a connection — closes its session and deletes its stored password. */
export function deleteConnection(id: string): Promise<void> {
  return picus('picus_delete_connection', { id });
}

export function connect(id: string): Promise<ConnectionStatus> {
  return picus('picus_connect', { id });
}

export function disconnect(id: string): Promise<void> {
  return picus('picus_disconnect', { id });
}

/**
 * Abandon a session that has stopped answering, and open a new one.
 *
 * Not the same as reconnecting. A session is one database connection, so a
 * statement the server will not stop blocks *everything* on it — including the
 * polite close that reconnecting begins with. This one drops the old connection
 * without sending it anything, which is the only thing that works once it has
 * proved it will not answer.
 */
export function resetConnection(id: string): Promise<ConnectionStatus> {
  return picus('picus_reset_connection', { id });
}

/** Which relation a result's rows came from, and what the catalogue calls it. */
export interface SourceRelation {
  /** Unqualified, spelled as the catalogue spells it. Empty when there is not
   *  exactly one source. */
  relation: string;
  /** The catalogue calls it a view — a source with no rows of its own. */
  isView: boolean;
  /** The catalogue has never heard of it: a CTE, a temporary table, an unread
   *  schema. */
  unknown: boolean;
  /** Why there is no single relation, in the user's terms. Empty when there is one. */
  reason: string;
}

/**
 * Trace a statement back to the relation it reads.
 *
 * Answered by Picus's SQL parser rather than by looking at the text, which is what
 * makes `EXTRACT(YEAR FROM x)` and a subquery in the select list stop counting as
 * extra sources — and what lets it say "that is a view" at all, since nothing in
 * the statement can know that.
 *
 * Pass the statement that **ran**, never the whole tab: a scratchpad holds several,
 * and the buffer as a whole reads from all of them.
 */
export function sourceRelation(connectionId: string, sql: string): Promise<SourceRelation> {
  return picus('picus_source_relation', { connectionId, sql });
}

/** Open, report, close. Deliberately does not touch the session pool. */
export function testConnection(connection: ConnectionSpec): Promise<ConnectionStatus> {
  return picus('picus_test_connection', { connection });
}

/** The application version from the project's version table (empty when absent). */
export function readDbVersion(
  id: string,
  table: string,
  column: string,
  filter: string,
): Promise<string> {
  return picus('picus_read_db_version', { id, table, column, filter });
}

// ── Passwords (shell-side — these never reach `picus-be`) ────────────────────

/** Store or replace a connection's password. An empty string deletes it. */
export function storeSecret(connectionId: string, secret: string): Promise<void> {
  return invoke('picus_store_secret', { connectionId, secret });
}

export function deleteSecret(connectionId: string): Promise<void> {
  return invoke('picus_delete_secret', { connectionId });
}

export function hasSecret(connectionId: string): Promise<boolean> {
  return invoke('picus_has_secret', { connectionId });
}

// ── Schema ───────────────────────────────────────────────────────────────────

export function readSchema(id: string): Promise<SchemaSnapshot> {
  return picus('picus_read_schema', { id });
}

/** One relation in full: constraints and indexes, paid for only when a tab opens. */
export function tableDetail(id: string, name: string): Promise<TableInfo> {
  return picus('picus_table_detail', { id, name });
}

/**
 * What a trigger does — its `CREATE TRIGGER` and the routine it fires.
 *
 * Lazy for the same reason `tableDetail` is: a routine body is far larger than the
 * facts beside it, and a schema with hundreds of triggers would carry every one of
 * them to answer a question asked about one.
 */
export function triggerDetail(id: string, name: string): Promise<TriggerDetail> {
  return picus('picus_trigger_detail', { id, name });
}

// ── Statements and held results ──────────────────────────────────────────────
//
// A read does not return "the rows": it returns a HELD CURSOR plus the first
// window onto it. Everything after that is `picus_result_window` against the same
// `resultId`, which is what makes scrolling a four-million-row table neither
// repeat a row nor skip one — an `OFFSET`/`LIMIT` pair over a table being written
// to does both.
//
// A cursor is a resource on someone's database. Every path that ends a result —
// a tab closing, a new statement replacing it, a connection going down — has to
// reach `picus_close_result`; the lifetime is owned by `stores/picus/result`.

/** The answer to any statement: a read opens a result, a write reports its count. */
export interface ExecuteResult {
  /** Handle of the held cursor. `null` for a statement that returns no rows. */
  resultId: string | null;
  columns: Column[];
  /** The first window — rows `[0, rowCount)` of the result. */
  rows: CellValue[][];
  /**
   * The planner's row estimate. It arrives with the first window, costs nothing,
   * and is wrong — anything displaying it must mark it approximate.
   */
  estimatedRows: number | null;
  /** The exact length, when the backend already had it without counting. */
  totalRows: number | null;
  /** Server-side elapsed time in ms. */
  elapsedMs: number;
  /** Rows in this first window. */
  rowCount: number;
  /** True when this window already reached the end — the length is then exact. */
  endOfResult: boolean;
  /** Rows a write touched. `null` for a read. */
  affected: number | null;
  /**
   * Columns whose value was **not fetched** — a large object, replaced in the
   * projection by its size in bytes.
   *
   * Empty for any statement the user wrote: Picus only rewrites a projection it
   * composed itself, which is the one a relation tab runs. A grid showing one of
   * these is showing a number where a value belongs and has to say so.
   */
  maskedColumns?: string[];
  /**
   * Columns present in every row but **hidden from the grid** — the row key Picus
   * spliced in so a masked cell could be addressed when the query did not select it.
   * They are the trailing columns, so hiding them is dropping the tail; `rowAt` and
   * the reveal still see them.
   */
  hiddenColumns?: string[];
  /**
   * The columns that identify one row, for reading a masked large object back —
   * the table's primary key (visible or hidden) or the engine's `ctid`. Empty when
   * the rows are not addressable.
   */
  rowKey?: string[];
  /**
   * The statement that actually ran, when it differs from the one sent — a key was
   * spliced into its projection, or its large objects were wrapped into sizes.
   * `undefined`/absent when what ran is what was asked for. Shown in the history so
   * "you asked X, Y ran" is never a surprise.
   */
  effectiveSql?: string;
}

/**
 * Run one statement. One door for every statement, read or write.
 *
 * `window` sizes the **first** window. Send the user's own "rows per window"
 * setting: every later window uses it, and a first window of a different size than
 * all the others is the kind of inconsistency nobody reports and everybody notices.
 * Omitted, the backend picks its default.
 */
export function execute(
  connectionId: string,
  sql: string,
  window?: number,
): Promise<ExecuteResult> {
  return picus('picus_execute', { connectionId, sql, window });
}

/**
 * One statement the server rejected, placed in the buffer.
 *
 * `start`/`end` are absolute UTF-8 byte offsets — the same coordinate the parse
 * faults use, so they feed the editor's lint layer without conversion.
 */
export interface ValidationFinding {
  start: number;
  end: number;
  message: string;
  code?: string;
}

/**
 * Validate a buffer against the connected database, without running it.
 *
 * Each preparable statement is prepared (parsed + described) on the server; whatever
 * it rejects comes back as a finding at the server's own position. Returns an empty
 * list when there is nothing to validate against (no session, an engine without the
 * capability); rejects only when the connection itself failed mid-check, so the
 * caller can tell "clean" from "could not ask".
 */
export function validateSql(connectionId: string, sql: string): Promise<ValidationFinding[]> {
  return picus('picus_validate', { id: connectionId, sql });
}

/** One statement in a buffer, addressed the way the editor addresses text. */
export interface StatementSpan {
  /** First UTF-16 code unit of the statement — a CodeMirror position. */
  start: number;
  /** One past the last. */
  end: number;
  /** 1-based line it starts on. */
  line: number;
  /** `select`, `insert`, `block`, … Labels a run; decides nothing. */
  kind: string;
}

/**
 * Where the statements are in a buffer.
 *
 * Asked of the backend rather than worked out here, and the reason is not tidiness:
 * a semicolon is a statement boundary *unless* it is inside a string literal, a
 * comment, a dollar-quoted body or an Oracle `DECLARE … BEGIN … END;` — and in
 * that last case there are several, none of which ends anything. A regular
 * expression gets all of those wrong, and a wrong answer here is half a statement
 * sent to a production database. `picus-parse` already knows, in both dialects.
 *
 * The offsets are **UTF-16 code units**, so they can be used as CodeMirror
 * positions directly. Never throws on half-typed SQL.
 */
export function sqlStatements(sql: string, dialect: Dialect): Promise<StatementSpan[]> {
  return picus('picus_sql_statements', { sql, dialect });
}

/** Open a relation's rows as a held result — the table tab's Data view. */
export function openRelation(
  connectionId: string,
  relation: string,
  window?: number,
): Promise<ExecuteResult> {
  return picus('picus_open_relation', { connectionId, relation, window });
}

export interface ResultWindow {
  /**
   * Echoes the requested offset. Load-bearing: a window that arrives after the
   * user has scrolled elsewhere is matched to its request by this, and dropped
   * when it belongs to a result that is no longer the one on screen.
   */
  offset: number;
  rows: CellValue[][];
  /** True when this window ran out — the result's length is then known exactly. */
  endOfResult: boolean;
}

export function resultWindow(
  connectionId: string,
  resultId: string,
  offset: number,
  limit: number,
): Promise<ResultWindow> {
  return picus('picus_result_window', { connectionId, resultId, offset, limit });
}

/**
 * The exact length of a held result.
 *
 * Slow by nature — it is the scan the estimate exists to avoid — so it is asked
 * for in the background and abortable through {@link cancel} like any other
 * statement on that connection.
 */
export function countResult(connectionId: string, resultId: string): Promise<{ total: number }> {
  return picus('picus_count_result', { connectionId, resultId });
}

/** Release the cursor. Idempotent: closing an already-closed result is not an error. */
export function closeResult(connectionId: string, resultId: string): Promise<void> {
  return picus('picus_close_result', { connectionId, resultId });
}

/** Ask the server to cancel. A no-op when nothing is running — never an error. */
export function cancel(id: string): Promise<void> {
  return picus('picus_cancel', { id });
}

// ── Generation (picus-emit) ──────────────────────────────────────────────────

/**
 * The dialect-free description of what to write — `picus_ast::DmlModel` on the
 * wire, field for field.
 *
 * It carries **no dialect**, deliberately: the dialect lives on each `Target`, so
 * one model becomes N statements, each correct on its own terms. Putting an engine
 * here would turn "the same change in both branches" from a guarantee into a
 * coincidence.
 */
export interface DmlModel {
  table: string;
  operation: DmlOperation;
  /** Full column set of the table — drives value formatting and ordering. */
  columns: Column[];
  /** The comparison key: what **identifies a row** — the conflict target of an
   *  upsert, the existence check of a guard, and what reconciliation matches on. */
  keyColumns: Column[];
  rows: DmlRow[];
  /**
   * The WHERE of an update or a delete, when it is more than "match the key".
   *
   * Separate from `keyColumns` on purpose: the key says *which row*, this says
   * *which rows*. It replaces the key-based WHERE rather than narrowing it —
   * AND-ing them would silently tighten a filter somebody wrote deliberately.
   */
  whereClause?: Predicate | null;
  /** Lowercase identifiers on PostgreSQL (a per-project convention). */
  lowercasePostgres: boolean;
  /** Where the installed version lives. `dateColumn: null` means the project
   *  stamps no date, and the closing UPDATE leaves the column out entirely. */
  versionTable: VersionTableConfig;
}

/** One destination's generated SQL, plus anything that makes its rules wrong. */
export interface EmittedTarget {
  targetId: string;
  sql: string;
  /**
   * Why this target's rules cannot all apply — e.g. a version guard on a target
   * that emits bare statements, which has nothing to return from. Absent when the
   * rule set is coherent; reported rather than silently dropped.
   */
  ruleConflict?: string;
}

/**
 * Generate the SQL for every target of one model.
 *
 * One call, N results — emitting per target in a loop here would be
 * re-implementing the product's central guarantee client-side.
 */
export function emit(model: DmlModel, targets: Target[]): Promise<EmittedTarget[]> {
  return picus('picus_emit', { model, targets });
}

/** One cell that cannot be written as typed. */
export interface ValueProblem {
  /** Index into the model's `rows`. */
  row: number;
  column: string;
  /** Why, in the user's terms — never a rule identifier. */
  reason: string;
}

/**
 * Check every supplied value against its column, in one round trip.
 *
 * Batched rather than per cell so the whole grid can be marked at once instead of
 * revealing its problems one at a time.
 */
export function validateRows(model: DmlModel): Promise<ValueProblem[]> {
  return picus('picus_validate_rows', { model });
}

/** Check a single value. `null` when it is writable as typed. */
export function validateValue(value: string, column: Column): Promise<string | null> {
  return picus('picus_validate_value', { value, column });
}

/** One large object, read on demand. */
export interface LobValue {
  /** The whole value's size in bytes — may exceed what came back. */
  bytes: number;
  /** The value, when the column holds text. */
  text?: string;
  /** The value base64-encoded, when the column holds bytes. */
  base64?: string;
  /** Only the beginning arrived; the rest is still on the server. */
  truncated: boolean;
}

/**
 * Read the value a masked cell stands for.
 *
 * A relation tab does not fetch its large objects — it fetches their sizes, so
 * opening a table of scanned documents does not pull every byte of every row across
 * the connection to draw a grid that cannot show any of them. This is the other half:
 * exactly the one value the user asked to see, addressed by its row's key, and
 * capped so that clicking a cell can never be what fills the window's memory.
 */
export function readLob(
  connectionId: string,
  table: string,
  keys: Record<string, string | null>,
  column: string,
): Promise<LobValue> {
  return picus('picus_read_lob', { id: connectionId, table, keys, column });
}

/** One row's worth of change: what identifies it, and what to write. */
export interface RowEdit {
  /** Key columns, with the values the row had **before** the edit. */
  keys: Record<string, string | null>;
  /** Columns to write, with their new values. */
  set: Record<string, string | null>;
}

/** What a batch of edits did. */
export interface EditOutcome {
  affected: number;
  requested: number;
  /** The SQL that ran, so it can be read — or pasted into a script. */
  sql: string;
  /** Set when the count and the request disagree, in the user's terms. */
  warning?: string;
}

/**
 * Write a grid's changed cells back to one table.
 *
 * The only call in Picus that issues DML the user has not read first, and it refuses
 * more than it accepts: no key, a NULL key, a view, or a read-only connection all
 * stop before anything is written. The `WHERE` is built from the values the row was
 * **read** with, which is what lets a key column itself be edited.
 */
export function applyRowEdits(
  connectionId: string,
  table: string,
  edits: RowEdit[],
): Promise<EditOutcome> {
  return picus('picus_apply_row_edits', { id: connectionId, table, edits });
}

/**
 * Rows out of a result grid, as `INSERT` statements for this connection's engine.
 *
 * Through the backend rather than joined together here, because **quoting is a
 * schema question**: whether `007` keeps its quotes and `15` loses them depends on
 * the column's declared type, which only the connection knows. It is also what
 * makes the answer differ correctly per engine — one statement per row on Oracle,
 * one statement with a tuple per row on PostgreSQL.
 *
 * A `null` cell means SQL `NULL`; the empty string means the empty string, and on
 * a text column those are different rows.
 */
export function rowsToInsert(
  connectionId: string,
  table: string,
  columns: string[],
  rows: (string | null)[][],
  dialect: Dialect,
): Promise<string> {
  return picus('picus_rows_to_insert', { id: connectionId, table, columns, rows, dialect });
}
