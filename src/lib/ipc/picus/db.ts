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
  Dialect,
  DmlOperation,
  DmlRow,
  SchemaGroup,
  SchemaSnapshot,
  TableInfo,
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
  /** The comparison key: the WHERE of updates, the existence check. */
  keyColumns: Column[];
  rows: DmlRow[];
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
