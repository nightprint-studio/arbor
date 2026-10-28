/**
 * Entity ⇄ table comparison — the "does this JPA mapping match that schema" check.
 *
 * Pure: the Java side comes from `bennu_persistence_model`, the database side from a Picus schema
 * snapshot, and the join happens here. Nothing in this file talks to a backend.
 *
 * ## Names are compared loosely, on purpose
 *
 * An entity names its table and columns through a naming strategy the project configures
 * (Spring Boot snake_cases `firstName`; plain Hibernate keeps it), and a legacy database adds its
 * own casing on top. Exact matching would flag every field of a project that is perfectly fine, so a
 * name is reduced to lowercase letters and digits: `FIRST_NAME`, `first_name` and `firstName` are
 * the same column. The cost is that two columns differing only by underscores collide — a schema
 * that does that has bigger problems than this check.
 */

import type { PersistenceEntity, PersistenceField } from '$lib/ipc/bennu/persistence';
import type { Column, TableInfo } from '$lib/types/picus';

/** The comparison key of a table or column name. */
export function nameKey(name: string): string {
  return name
    .replace(/^.*\./, '') // drop a schema prefix
    .replace(/["`\[\]]/g, '')
    .replace(/[^A-Za-z0-9]/g, '')
    .toLowerCase();
}

/** `OrderItem` → `order_item`. */
export function toSnake(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2')
    .toLowerCase();
}

export type TypeFamily = 'text' | 'integer' | 'decimal' | 'boolean' | 'temporal' | 'binary' | 'uuid' | 'other';

const JAVA_FAMILIES: Record<string, TypeFamily> = {
  String: 'text', char: 'text', Character: 'text',
  int: 'integer', Integer: 'integer', long: 'integer', Long: 'integer', short: 'integer', Short: 'integer',
  BigInteger: 'integer', byte: 'integer', Byte: 'integer',
  BigDecimal: 'decimal', double: 'decimal', Double: 'decimal', float: 'decimal', Float: 'decimal',
  boolean: 'boolean', Boolean: 'boolean',
  Date: 'temporal', LocalDate: 'temporal', LocalDateTime: 'temporal', LocalTime: 'temporal',
  OffsetDateTime: 'temporal', ZonedDateTime: 'temporal', Instant: 'temporal', Timestamp: 'temporal',
  Calendar: 'temporal',
  UUID: 'uuid',
};

/** The family of a Java type; `other` for anything the check cannot judge (enums, embeddables, …). */
export function javaFamily(typeText: string): TypeFamily {
  if (typeText.endsWith('[]')) return 'binary';
  return JAVA_FAMILIES[typeText] ?? 'other';
}

/** The family of a native column type (`VARCHAR2(30)`, `numeric(5,2)`, `timestamptz`). */
export function sqlFamily(type: string): TypeFamily {
  const t = type.toLowerCase();
  if (/(char|text|clob|string|json|xml|enum)/.test(t)) return 'text';
  if (/(bool|bit)/.test(t)) return 'boolean';
  if (/(timestamp|date|time|interval)/.test(t)) return 'temporal';
  if (/(uuid|uniqueidentifier)/.test(t)) return 'uuid';
  if (/(bytea|blob|binary|raw)/.test(t)) return 'binary';
  if (/(numeric|decimal|number|float|double|real|money)/.test(t)) {
    // Oracle's NUMBER(10) / numeric(10,0) is an integer in all but name.
    const scale = /\(\s*\d+\s*(?:,\s*(\d+))?\s*\)/.exec(t);
    if (/^(number|numeric|decimal)/.test(t) && scale && (scale[1] === undefined || Number(scale[1]) === 0)) {
      return 'integer';
    }
    return 'decimal';
  }
  if (/(int|serial)/.test(t)) return 'integer';
  return 'other';
}

/** Whether a Java family can be stored in a SQL family without the provider objecting. */
function compatible(java: TypeFamily, sql: TypeFamily): boolean {
  if (java === 'other' || sql === 'other') return true;
  if (java === sql) return true;
  // An integer fits a decimal column, and Oracle stores booleans as NUMBER(1).
  if (java === 'integer' && sql === 'decimal') return true;
  if (java === 'decimal' && sql === 'integer') return true;
  if (java === 'boolean' && (sql === 'integer' || sql === 'decimal')) return true;
  // A UUID is commonly kept as text in a database with no native type.
  if (java === 'uuid' && sql === 'text') return true;
  // `@Lob String` over a CLOB, and enums over text, already land in `other`/`text`.
  return false;
}

export type EntityStatus = 'ok' | 'drift' | 'missing-table';

export interface EntityProblem {
  kind: 'missing-column' | 'type-mismatch' | 'unmapped-column';
  message: string;
  /** Source line of the field, when the problem is about one. */
  line?: number;
}

export interface EntityCheck {
  entity: PersistenceEntity;
  /** The table the entity resolves to, as the mapping spells it. */
  tableName: string;
  /** The matched table, when the schema has it. */
  table: TableInfo | null;
  status: EntityStatus;
  problems: EntityProblem[];
}

/** The table an entity is stored in: `@Table(name)` when written, else the snake-cased class name. */
export function tableNameOf(entity: PersistenceEntity): string {
  return entity.table || toSnake(entity.simple);
}

function columnNameOf(field: PersistenceField): string {
  return field.column || toSnake(field.name);
}

function compareFields(entity: PersistenceEntity, table: TableInfo): EntityProblem[] {
  const problems: EntityProblem[] = [];
  const byKey = new Map<string, Column>(table.columns.map((c) => [nameKey(c.name), c]));
  const mapped = new Set<string>();

  // Whether every column an entity can own has been accounted for. When something might own a column
  // this check cannot name — an embedded object, a foreign key whose name is neither written nor
  // guessable — "this column is not mapped" would be a claim it cannot make, so it is not made.
  let canListUnmapped = true;

  for (const field of entity.fields) {
    if (field.embedded) {
      canListUnmapped = false;
      continue;
    }
    if (field.is_relation) {
      if (!field.owns_column) continue; // the other side holds the key, or a join table does
      const written = field.join_column;
      const guess = `${field.name}_id`;
      const col = byKey.get(nameKey(written || guess));
      if (written) {
        mapped.add(nameKey(written));
        if (!col) {
          problems.push({
            kind: 'missing-column',
            message: `${field.name} → no join column "${written}" in ${table.name}`,
            line: field.line,
          });
        }
      } else if (col) {
        mapped.add(nameKey(guess));
      } else {
        // The default name did not match: the project's naming strategy is something this does not know.
        canListUnmapped = false;
      }
      continue;
    }
    const key = nameKey(columnNameOf(field));
    mapped.add(key);
    const col = byKey.get(key);
    if (!col) {
      problems.push({
        kind: 'missing-column',
        message: `${field.name} → no column "${columnNameOf(field)}" in ${table.name}`,
        line: field.line,
      });
      continue;
    }
    // A converter is what bridges a Java type and a column of a different one (a `Boolean` kept in a
    // `varchar`), so the pair is the author's to judge, not this check's.
    if (!field.converted && !compatible(javaFamily(field.type_text), sqlFamily(col.type))) {
      problems.push({
        kind: 'type-mismatch',
        message: `${field.name}: ${field.type_text} over ${col.name} ${col.type}`,
        line: field.line,
      });
    }
  }

  if (canListUnmapped) {
    for (const col of table.columns) {
      if (!mapped.has(nameKey(col.name))) {
        problems.push({ kind: 'unmapped-column', message: `column ${col.name} (${col.type}) is not mapped` });
      }
    }
  }
  return problems;
}

/**
 * Check every entity against `tables`. A `mapped-superclass` has no table of its own and is left
 * out; so is an entity the schema simply does not contain a view of (reported as `missing-table`,
 * which is a finding, not a failure to check).
 */
export function checkEntities(entities: PersistenceEntity[], tables: TableInfo[]): EntityCheck[] {
  const byKey = new Map<string, TableInfo>(tables.map((t) => [nameKey(t.name), t]));
  return entities
    .filter((e) => e.kind === 'entity')
    .map((entity) => {
      const tableName = tableNameOf(entity);
      const table = byKey.get(nameKey(tableName)) ?? null;
      if (!table) {
        return { entity, tableName, table, status: 'missing-table' as const, problems: [] };
      }
      const problems = compareFields(entity, table);
      // An unmapped column is information — a legacy table has columns an entity never needed.
      const real = problems.some((p) => p.kind !== 'unmapped-column');
      return { entity, tableName, table, status: real ? ('drift' as const) : ('ok' as const), problems };
    });
}
