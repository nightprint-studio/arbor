/**
 * Bennu persistence-model IPC — what a project's JPA layer declares, for checking against a live
 * database (Picus). The database half lives in `$lib/ipc/picus`; this is only the Java side.
 *
 * Wire: `bennu_persistence_model` — `RootArgs { root }`. Snake_case, as the JPA form model is.
 */

import { bennu } from '../rpc';

/** One mapped (non-transient) field of an entity. */
export interface PersistenceField {
  name: string;
  /** Simple type name (`Long`, `String`, `LocalDate`). */
  type_text: string;
  /** `@Column(name = …)` when written; empty means the naming strategy decides. */
  column: string;
  is_id: boolean;
  /** A relation owns a join column its name says nothing about; the check skips it. */
  is_relation: boolean;
  /** A to-one relation that holds the foreign key: it owns a column despite `is_relation`. */
  owns_column: boolean;
  /** `@JoinColumn(name = …)` of such a relation; empty when the provider defaults it. */
  join_column: string;
  /** Stored through an `AttributeConverter`, so its Java type says nothing about the column's. */
  converted: boolean;
  /** An embedded object: its columns are the embeddable's. */
  embedded: boolean;
  line: number;
}

export interface PersistenceEntity {
  fqcn: string;
  simple: string;
  /** `@Table(name = …)`, empty when defaulted. */
  table: string;
  /** `entity` | `mapped-superclass` (embeddables are not tables and are not sent). */
  kind: string;
  file: string;
  line: number;
  fields: PersistenceField[];
}

/** A `@Query(nativeQuery = true)` on a Spring Data repository. */
export interface NativeQuerySite {
  repository: string;
  method: string;
  text: string;
  file: string;
  line: number;
  named_params: string[];
  positional_params: number[];
}

export interface PersistenceModel {
  /** The project has JPA / Hibernate; without it the entity tooling has nothing to work on. */
  jpa: boolean;
  entities: PersistenceEntity[];
  native_queries: NativeQuerySite[];
}

export function persistenceModel(root: string): Promise<PersistenceModel> {
  return bennu('bennu_persistence_model', { args: { root } });
}

/** A native SQL string in a Java buffer: its text, and where its contents sit (UTF-8 byte offsets). */
export interface NativeSqlSpan {
  start: number;
  end: number;
  text: string;
}

/** The native SQL strings of the live buffer — `@Query(nativeQuery = true)`, `createNativeQuery`,
 *  `@NamedNativeQuery`, and any string a `// language=sql` comment names. Wire: `bennu_native_sql_sites`. */
export function nativeSqlSites(file: string, source: string): Promise<NativeSqlSpan[]> {
  return bennu('bennu_native_sql_sites', { args: { file, source } });
}

/** One statement of the mapper in a live buffer, with its opening tag as a byte span. */
export interface MybatisFileStatement {
  id: string;
  kind: string;
  database_id: string;
  sql: string;
  unchecked: string | null;
  start: number;
  end: number;
}

/** The statements of one mapper, rendered from the editor's text. Wire: `bennu_mybatis_file_statements`. */
export function mybatisFileStatements(source: string, values: [string, string][] = []): Promise<MybatisFileStatement[]> {
  return bennu('bennu_mybatis_file_statements', { args: { source, values } });
}

/** One MyBatis mapper statement rendered as plain SQL (dynamic tags flattened, bindings as `NULL`).
 *  Wire: `bennu_mybatis_statements`. */
export interface MybatisStatement {
  file: string;
  namespace: string;
  id: string;
  /** `select` | `insert` | `update` | `delete`. */
  kind: string;
  line: number;
  /** The `databaseId` attribute; empty means every database. */
  database_id: string;
  sql: string;
  /** Dynamic tags were involved, so `sql` is one expansion among several. */
  dynamic: boolean;
  /** Why it cannot be prepared (`${…}` splicing, a fragment from another mapper). */
  unchecked: string | null;
  /** The `${name}` splices it contains, whether or not they were given a value. */
  splices: string[];
}

/** `values` says what each `${name}` stands for, as `[name, value]` pairs. */
export function mybatisStatements(root: string, values: [string, string][] = []): Promise<MybatisStatement[]> {
  return bennu('bennu_mybatis_statements', { args: { root, values } });
}
