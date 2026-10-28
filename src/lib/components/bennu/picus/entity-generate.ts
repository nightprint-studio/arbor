/**
 * Table → JPA entity source.
 *
 * Pure text generation from a Picus `TableInfo`: a POJO with `@Entity`, `@Table`, `@Id`, explicit
 * `@Column(name = …)` and accessors (written, or Lombok's, bean or fluent). A single-column foreign key
 * becomes a `@ManyToOne` placed after the plain columns; a composite one stays scalar columns —
 * that is a modelling decision (@MapsId / @JoinColumns), not a transcription. `@Column(name)` is
 * always written, because whether a bare field would resolve to the right column depends on a naming
 * strategy this file cannot see.
 */

import type { Column, TableInfo } from '$lib/types/picus';
import { sqlFamily } from './entity-check';

export type PersistenceApi = 'jakarta' | 'javax';

export interface EntityGenOptions {
  /** Package declaration; empty writes none. */
  packageName: string;
  api: PersistenceApi;
  /** `@Getter`/`@Setter` instead of written accessors. */
  lombok: boolean;
  /** `name()` / `name(v)` returning `this`, instead of `getName()` / `setName(v)`. */
  fluent: boolean;
  /** A single-column foreign key becomes a `@ManyToOne` instead of a plain column. */
  associations: boolean;
  /** `@GeneratedValue` on a key the database fills in (a `serial`, a sequence default). */
  generatedIds: boolean;
  /** The simple name of the project's entity for a table, or `null` when it has none yet. */
  entityOfTable: (table: string) => string | null;
  /** Read-only entity: Hibernate's `@Immutable`, and no setters. */
  immutable: boolean;
  /** Per foreign-key column, how its association is written — see {@link JoinTweak}. */
  joins: Record<string, Partial<JoinTweak>>;
}

export const CASCADE_KINDS = ['PERSIST', 'MERGE', 'REMOVE', 'REFRESH', 'DETACH', 'ALL'] as const;
export type CascadeKind = (typeof CASCADE_KINDS)[number];

/** What can be adjusted on a generated `@ManyToOne` — the choices that vary per relation. */
export interface JoinTweak {
  fetch: 'lazy' | 'eager';
  /** JPA cascade types; empty writes none. */
  cascade: CascadeKind[];
  /** What the database does to this row when the referenced one is deleted: Hibernate's `@OnDelete`. */
  onDelete: 'none' | 'cascade' | 'set-null';
}

export const DEFAULT_JOIN: JoinTweak = { fetch: 'lazy', cascade: [], onDelete: 'none' };

/** A single-column foreign key of a table that can become an association. */
export interface ForeignKeyAssociation {
  column: string;
  /** The class it points at: the project's entity for the table, or a name made from it. */
  target: string;
  /** Whether the project has an entity for the referenced table. */
  known: boolean;
  notNull: boolean;
}

/**
 * The foreign keys of `table` that become an association. A single-column key that is not part of
 * the primary key does; anything else (a composite key, a key column) stays a plain column — turning
 * those into associations needs @MapsId / @JoinColumns decisions that are the author's to make.
 */
export function foreignKeyAssociations(table: TableInfo, entityOfTable: (table: string) => string | null): ForeignKeyAssociation[] {
  const out: ForeignKeyAssociation[] = [];
  for (const fk of table.foreignKeys ?? []) {
    if (fk.columns.length !== 1 || fk.referencedColumns.length !== 1) continue;
    const column = table.columns.find((c) => c.name === fk.columns[0]);
    if (!column || column.primaryKey) continue;
    const existing = entityOfTable(fk.referencedTable);
    out.push({
      column: column.name,
      target: existing ?? (toPascal(fk.referencedTable) || 'Object'),
      known: existing !== null,
      notNull: !!column.notNull,
    });
  }
  return out;
}

interface JavaType {
  name: string;
  /** Fully-qualified import, when the type is not `java.lang`. */
  imports?: string;
  lob?: boolean;
}

/** `ORDER_ITEM` / `order_item` → `OrderItem`. */
export function toPascal(name: string): string {
  return name
    .replace(/^.*\./, '')
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1).toLowerCase())
    .join('');
}

/** `first_name` → `firstName`; a Java keyword or a leading digit gets an underscore. */
export function toCamel(name: string): string {
  const p = toPascal(name);
  const camel = p ? p[0].toLowerCase() + p.slice(1) : 'field';
  return JAVA_RESERVED.has(camel) || /^\d/.test(camel) ? `_${camel}` : camel;
}

const JAVA_RESERVED = new Set([
  'abstract', 'assert', 'boolean', 'break', 'byte', 'case', 'catch', 'char', 'class', 'const', 'continue',
  'default', 'do', 'double', 'else', 'enum', 'extends', 'final', 'finally', 'float', 'for', 'goto', 'if',
  'implements', 'import', 'instanceof', 'int', 'interface', 'long', 'native', 'new', 'package', 'private',
  'protected', 'public', 'return', 'short', 'static', 'strictfp', 'super', 'switch', 'synchronized', 'this',
  'throw', 'throws', 'transient', 'try', 'void', 'volatile', 'while',
]);

const T = {
  string: { name: 'String' },
  integer: { name: 'Integer' },
  long: { name: 'Long' },
  short: { name: 'Short' },
  decimal: { name: 'BigDecimal', imports: 'java.math.BigDecimal' },
  double: { name: 'Double' },
  float: { name: 'Float' },
  bool: { name: 'Boolean' },
  date: { name: 'LocalDate', imports: 'java.time.LocalDate' },
  time: { name: 'LocalTime', imports: 'java.time.LocalTime' },
  dateTime: { name: 'LocalDateTime', imports: 'java.time.LocalDateTime' },
  offset: { name: 'OffsetDateTime', imports: 'java.time.OffsetDateTime' },
  uuid: { name: 'UUID', imports: 'java.util.UUID' },
  bytes: { name: 'byte[]' },
  clob: { name: 'String', lob: true },
  blob: { name: 'byte[]', lob: true },
} satisfies Record<string, JavaType>;

/** The Java type a column's native type maps to. */
export function javaTypeOf(column: Column): JavaType {
  const t = column.type.toLowerCase();
  if (/(clob|^text$|longtext)/.test(t)) return T.clob;
  if (/(blob|bytea|binary|^raw)/.test(t)) return /blob/.test(t) ? T.blob : T.bytes;
  if (/timestamptz|with time zone/.test(t)) return T.offset;
  if (/timestamp/.test(t)) return T.dateTime;
  if (/^date/.test(t)) return T.date;
  if (/^time/.test(t)) return T.time;
  if (/(^bool|^bit$)/.test(t)) return T.bool;
  if (/^uuid/.test(t)) return T.uuid;
  if (/^(smallint|int2|smallserial)/.test(t)) return T.short;
  if (/^(bigint|int8|bigserial)/.test(t)) return T.long;
  if (/^(int|integer|int4|serial|mediumint)/.test(t)) return T.integer;
  if (/^(float4|real)/.test(t)) return T.float;
  if (/^(float8|double|float)/.test(t)) return T.double;
  const family = sqlFamily(t);
  if (family === 'integer') return integerForPrecision(t);
  if (family === 'decimal') return T.decimal;
  if (family === 'text') return T.string;
  return T.string;
}

/** `NUMBER(p)` / `numeric(p,0)`: an `Integer` up to 9 digits, a `Long` up to 18, else a `BigDecimal`. */
function integerForPrecision(type: string): JavaType {
  const p = /\(\s*(\d+)/.exec(type);
  if (!p) return T.long;
  const digits = Number(p[1]);
  if (digits <= 9) return T.integer;
  return digits <= 18 ? T.long : T.decimal;
}

/** `varchar(30)` → 30; undefined when the type declares no length or it is the JPA default. */
function lengthOf(type: string): number | undefined {
  if (!/char|string/i.test(type)) return undefined;
  const m = /\(\s*(\d+)/.exec(type);
  const n = m ? Number(m[1]) : undefined;
  return n && n !== 255 ? n : undefined;
}

function columnAnnotation(column: Column, java: JavaType): string {
  const parts = [`name = "${column.name}"`];
  if (column.notNull && !column.primaryKey) parts.push('nullable = false');
  if (!java.lob) {
    const length = lengthOf(column.type);
    if (length) parts.push(`length = ${length}`);
  }
  return `@Column(${parts.join(', ')})`;
}

function accessor(prefix: 'get' | 'set', field: string): string {
  return prefix + field[0].toUpperCase() + field.slice(1);
}

/** Getter and setter of one field, in the bean style or the fluent one (setter returns `this`).
 *  A read-only entity gets the getter alone. */
function accessorsOf(className: string, type: string, name: string, fluent: boolean, readOnly: boolean): string[] {
  const all = fluent
    ? [
        `    public ${type} ${name}() {\n        return ${name};\n    }`,
        `    public ${className} ${name}(${type} ${name}) {\n        this.${name} = ${name};\n        return this;\n    }`,
      ]
    : [
        `    public ${type} ${accessor('get', name)}() {\n        return ${name};\n    }`,
        `    public void ${accessor('set', name)}(${type} ${name}) {\n        this.${name} = ${name};\n    }`,
      ];
  return readOnly ? all.slice(0, 1) : all;
}

/** `CascadeType.ALL`, or `{ CascadeType.PERSIST, CascadeType.MERGE }`. */
function cascadeText(kinds: CascadeKind[]): string {
  const named = kinds.map((k) => `CascadeType.${k}`);
  return named.length === 1 ? named[0] : `{ ${named.join(', ')} }`;
}

/** How an `@Id` column gets its value, when the database does. */
type Generation =
  | { kind: 'identity' }
  | { kind: 'sequence'; sequence: string };

/**
 * Whether the database fills `column` in itself — a `serial`, or a default drawn from a sequence.
 * Only for a single-column key: a composite key is assigned by the application, never generated.
 * A default that is anything else (an expression, a constant) says nothing about generation.
 */
export function generationOf(table: TableInfo, column: Column): Generation | null {
  if (!column.primaryKey || table.columns.filter((c) => c.primaryKey).length !== 1) return null;
  if (/serial/i.test(column.type)) return { kind: 'identity' };
  const next = /nextval\(\s*'?"?([\w.$]+)/i.exec(column.defaultValue ?? '');
  if (!next) return null;
  const sequence = next[1].replace(/^.*\./, '');
  // `<table>_<column>_seq` is what `serial` itself creates: IDENTITY is the faithful mapping.
  return sequence.toLowerCase() === `${table.name}_${column.name}_seq`.toLowerCase()
    ? { kind: 'identity' }
    : { kind: 'sequence', sequence };
}

/** `customer_id` → `customer`; a name with no `_id` keeps itself, plus `Ref` when it would clash. */
function associationName(column: string, taken: Set<string>): string {
  const stripped = column.replace(/_?id$/i, '');
  const base = toCamel(stripped || column);
  return taken.has(base) ? `${base}Ref` : base;
}

interface FieldPlan {
  /** What goes above the field. */
  annotations: string[];
  type: string;
  name: string;
  /** The column behind it, when it is a plain one — what a composite key's nested class copies. */
  column?: Column;
  /** A `@ManyToOne` made from a foreign key — placed after the plain columns. */
  association?: boolean;
  note?: string;
}

/** The Java source of an entity for `table`. */
export function generateEntity(table: TableInfo, opts: EntityGenOptions): { className: string; source: string } {
  const className = toPascal(table.name) || 'Entity';
  const pk = table.columns.filter((c) => c.primaryKey);
  const composite = pk.length > 1;
  const api = opts.api;
  const imports = new Set<string>(['Column', 'Entity', 'Id', 'Table'].map((n) => `${api}.persistence.${n}`));

  // A single-column foreign key that is not part of the key becomes an association; anything else
  // (a composite foreign key, a key column) stays a plain column — turning those into associations
  // needs @MapsId / @JoinColumns decisions that are the author's to make.
  const associations = new Map<string, ForeignKeyAssociation>();
  if (opts.associations) {
    for (const a of foreignKeyAssociations(table, opts.entityOfTable)) associations.set(a.column, a);
  }

  const taken = new Set(table.columns.filter((c) => !associations.has(c.name)).map((c) => toCamel(c.name)));
  const inColumnOrder: FieldPlan[] = table.columns.map((column) => {
    const assoc = associations.get(column.name);
    if (assoc) {
      const join = { ...DEFAULT_JOIN, ...opts.joins[column.name] };
      imports.add(`${api}.persistence.ManyToOne`);
      imports.add(`${api}.persistence.JoinColumn`);
      imports.add(`${api}.persistence.FetchType`);
      const name = associationName(column.name, taken);
      taken.add(name);
      const relation = [`fetch = FetchType.${join.fetch.toUpperCase()}`];
      if (join.cascade.length) {
        imports.add(`${api}.persistence.CascadeType`);
        relation.push(`cascade = ${cascadeText(join.cascade)}`);
      }
      const annotations = [
        `@ManyToOne(${relation.join(', ')})`,
        `@JoinColumn(name = "${column.name}"${assoc.notNull ? ', nullable = false' : ''})`,
      ];
      if (join.onDelete !== 'none') {
        imports.add('org.hibernate.annotations.OnDelete');
        imports.add('org.hibernate.annotations.OnDeleteAction');
        annotations.push(`@OnDelete(action = OnDeleteAction.${join.onDelete === 'cascade' ? 'CASCADE' : 'SET_NULL'})`);
      }
      return {
        annotations,
        type: assoc.target,
        name,
        association: true,
        note: assoc.known ? undefined : `no entity for the table it references yet — create ${assoc.target}`,
      };
    }
    const java = javaTypeOf(column);
    if (java.imports) imports.add(java.imports);
    if (java.lob) imports.add(`${api}.persistence.Lob`);
    const annotations: string[] = [];
    if (column.primaryKey) annotations.push('@Id');
    const generated = opts.generatedIds ? generationOf(table, column) : null;
    if (generated?.kind === 'identity') {
      imports.add(`${api}.persistence.GeneratedValue`);
      imports.add(`${api}.persistence.GenerationType`);
      annotations.push('@GeneratedValue(strategy = GenerationType.IDENTITY)');
    } else if (generated?.kind === 'sequence') {
      imports.add(`${api}.persistence.GeneratedValue`);
      imports.add(`${api}.persistence.GenerationType`);
      imports.add(`${api}.persistence.SequenceGenerator`);
      annotations.push(
        `@GeneratedValue(strategy = GenerationType.SEQUENCE, generator = "${generated.sequence}")`,
        `@SequenceGenerator(name = "${generated.sequence}", sequenceName = "${generated.sequence}", allocationSize = 1)`,
      );
    }
    if (java.lob) annotations.push('@Lob');
    annotations.push(columnAnnotation(column, java));
    return { annotations, type: java.name, name: toCamel(column.name), column };
  });
  // Associations go after every column, before the accessors: the columns read as the table does and
  // the relations as one block, which is how an entity is usually kept tidy by hand.
  const plans = [...inColumnOrder.filter((p) => !p.association), ...inColumnOrder.filter((p) => p.association)];

  if (opts.lombok) {
    imports.add('lombok.Getter');
    if (!opts.immutable) imports.add('lombok.Setter');
    if (opts.fluent) imports.add('lombok.experimental.Accessors');
  }
  const keyClass = `${className}Key`;
  if (composite) {
    imports.add(`${api}.persistence.IdClass`);
    imports.add('java.io.Serializable');
    if (!opts.lombok) imports.add('java.util.Objects');
    else imports.add('lombok.EqualsAndHashCode');
  }

  const body = plans.map((p) => {
    const lines = [...(p.note ? [`    // ${p.note}`] : []), ...p.annotations.map((a) => `    ${a}`)];
    lines.push(`    private ${p.type} ${p.name};`);
    return lines.join('\n');
  });

  // With Lombok the accessors are generated at compile time, so none are written.
  const accessors = opts.lombok
    ? []
    : plans.flatMap((p) => accessorsOf(className, p.type, p.name, opts.fluent, opts.immutable));
  const lombokAnnotations = opts.lombok
    ? ['@Getter', ...(opts.immutable ? [] : ['@Setter']), ...(opts.fluent ? ['@Accessors(fluent = true)'] : [])]
    : [];
  if (opts.immutable) imports.add('org.hibernate.annotations.Immutable');

  const head: string[] = [];
  if (opts.packageName) head.push(`package ${opts.packageName};`, '');
  head.push(...[...imports].sort().map((i) => `import ${i};`), '');

  const note = pk.length === 0
    ? '// This table declares no primary key; JPA requires an @Id — pick the field that identifies a row.\n'
    : '';
  const idClass = composite ? [`@IdClass(${className}.${keyClass}.class)`] : [];

  const source = [
    ...head,
    `${note}@Entity`,
    ...(opts.immutable ? ['@Immutable'] : []),
    `@Table(name = "${table.name}")`,
    ...idClass,
    ...lombokAnnotations,
    `public class ${className} {`,
    '',
    body.join('\n\n'),
    ...(accessors.length ? ['', accessors.join('\n\n')] : []),
    ...(composite ? ['', keyClassSource(keyClass, plans.filter((p) => p.column?.primaryKey), opts.lombok)] : []),
    '}',
    '',
  ].join('\n');
  return { className, source };
}

/** The `@IdClass` of a composite key: the key fields again, `Serializable`, with equals and hashCode. */
function keyClassSource(name: string, fields: FieldPlan[], lombok: boolean): string {
  const decl = fields.map((f) => `        private ${f.type} ${f.name};`).join('\n');
  if (lombok) {
    return [
      '    @Getter',
      '    @Setter',
      '    @EqualsAndHashCode',
      `    public static class ${name} implements Serializable {`,
      decl,
      '    }',
    ].join('\n');
  }
  const names = fields.map((f) => f.name);
  const equals = names.map((n) => `Objects.equals(${n}, that.${n})`).join('\n                && ');
  return [
    `    public static class ${name} implements Serializable {`,
    decl,
    '',
    '        @Override',
    '        public boolean equals(Object o) {',
    '            if (this == o) return true;',
    `            if (!(o instanceof ${name})) return false;`,
    `            ${name} that = (${name}) o;`,
    `            return ${equals};`,
    '        }',
    '',
    '        @Override',
    '        public int hashCode() {',
    `            return Objects.hash(${names.join(', ')});`,
    '        }',
    '    }',
  ].join('\n');
}
