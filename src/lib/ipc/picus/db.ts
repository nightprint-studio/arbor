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
  SchemaGroup,
  SchemaSnapshot,
  TableInfo,
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

export interface RowPage {
  columns: Column[];
  rows: CellValue[][];
  offset: number;
  /** Row estimate when cheaply available — never a `count(*)` over a huge table. */
  total?: number;
}

export function fetchPage(
  id: string,
  name: string,
  offset: number,
  limit: number,
): Promise<RowPage> {
  return picus('picus_fetch_page', { id, name, offset, limit });
}

// ── Statements ───────────────────────────────────────────────────────────────

export interface ExecuteResult {
  columns: Column[];
  rows: CellValue[][];
  elapsedMs: number;
  rowCount: number;
  truncated: boolean;
  commandTag: string;
}

export function execute(id: string, sql: string, limit: number): Promise<ExecuteResult> {
  return picus('picus_execute', { id, sql, limit });
}

/** Ask the server to cancel. A no-op when nothing is running — never an error. */
export function cancel(id: string): Promise<void> {
  return picus('picus_cancel', { id });
}
