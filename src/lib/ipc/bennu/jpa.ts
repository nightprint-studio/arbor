/**
 * Bennu JPA generation IPC.
 *
 * The read-only side of the JPA extension (highlights, gutter, hover, catalogs) goes through the
 * generic `ext.ts` surface like every framework does — a second framework costs no second IPC
 * file. What lives here is the half the generic seam has no verb for and should not grow one:
 * **generating Java source**, which is not a question about a caret.
 *
 * Two calls. One describes what can be generated, so the form renders itself from the project
 * rather than from a hard-coded list; the other produces the text. Neither writes to disk — the
 * frontend applies the result through the ordinary edit path, so a generation is undoable like
 * any other edit instead of being a special case.
 */

import { bennu } from '../rpc';

/** A property path a query can address, with the type it resolves to. */
export interface JpaProperty {
  /** `total`, `customer.name`. */
  path: string;
  type_text: string;
}

export interface JpaEntityView {
  fqcn: string;
  simple: string;
  file: string;
  /** `entity` | `embeddable` | `mapped-superclass`. Sent rather than filtered on: an attribute
   *  can be added to any of the three, a repository only to the first. */
  kind: string;
  properties: JpaProperty[];
  has_repository: boolean;
}

export interface JpaRepoView {
  fqcn: string;
  simple: string;
  entity: string;
  file: string;
}

/** Everything the generation form renders itself from. */
export interface JpaFormModel {
  entities: JpaEntityView[];
  repositories: JpaRepoView[];
  /** `[verb, what it does]`. */
  subjects: [string, string][];
  /** The comparison vocabulary, each with how many arguments it binds. */
  keywords: JpaKeyword[];
  /** `[event, when it fires]` — the seven JPA lifecycle callbacks. */
  lifecycle: [string, string][];
  /** The relation annotations an attribute may carry. */
  relations: string[];
}

/** One comparison the query builder offers.
 *
 *  `args` is what lets the form say *what* a condition compares against — `Between` binds two
 *  parameters, `IsNull` binds none, everything else binds one. Without it the row read
 *  "not equal to" and left the rest of the sentence to the imagination. */
export interface JpaKeyword {
  /** `''` is plain equality. */
  keyword: string;
  label: string;
  args: number;
  /** The single argument is a collection (`In` / `NotIn`) — the generated parameter is plural. */
  collection: boolean;
}

/** One condition of a query being built. */
export interface JpaCondition {
  path: string;
  keyword: string;
  ignore_case: boolean;
  /** Joined to the previous condition with `Or` rather than `And`. */
  or: boolean;
}

export interface JpaQuerySpec {
  /** A hand-written name instead of the derived one. Empty = derived. An overridden name is no
   *  longer resolvable from the name, so the backend emits the `@Query` alongside it. */
  name: string;
  /** Write the `@Query` out even though the derived name would resolve on its own — for the
   *  joins and fetches a name cannot express. Forced on by `name`. */
  with_query: boolean;
  subject: string;
  distinct: boolean;
  limit: number | null;
  conditions: JpaCondition[];
  /** `[path, 'asc' | 'desc']`. */
  order_by: [string, string][];
  /** What the finder hands back. One choice, not two flags — `slice` and `stream` had nowhere to
   *  live in the `many`/`paged` pair this replaces, and neither did "a single result that isn't
   *  wrapped in Optional", which is what half of a legacy codebase's finders look like. */
  returns: JpaReturnShape;
  /** Take a `Sort` parameter, so the caller decides the ordering. Dropped on a paged method — its
   *  `Pageable` already carries a `Sort`, and a method taking both does not compile. */
  sorted: boolean;
  projection: string;
}

/** The shapes a finder can return. */
export type JpaReturnShape = 'optional' | 'single' | 'list' | 'page' | 'slice' | 'stream';

/** `[value, label, what it means]` — the return-type row, in the order it reads. */
export const JPA_RETURN_SHAPES: [JpaReturnShape, string, string][] = [
  ['optional', 'Optional', 'Optional<E> — one row or none'],
  ['single', 'Single', 'E — one row, or null'],
  ['list', 'List', 'List<E> — every match, held in memory'],
  ['page', 'Page', 'Page<E> + Pageable — the rows and the total count'],
  ['slice', 'Slice', 'Slice<E> + Pageable — the rows and whether more follow, with no count(*)'],
  ['stream', 'Stream', 'Stream<E> — too many rows to hold; the caller closes it'],
];

export interface JpaInsertion {
  file: string;
  /** Byte offset in that file's current text. */
  offset: number;
  text: string;
}

/** What a generation produced. Both halves when both are honest — a projection is genuinely
 *  either a file of its own or an interface nested in the repository. */
export interface JpaGenerated {
  /** `[path, content]` for a file to create. */
  file: [string, string] | null;
  insertion: JpaInsertion | null;
  preview: string;
  /** The `alter table` the change implies, for the preview's second tab. Empty when there is none
   *  to write honestly. A starting point, not a migration: no dialect, and no back-fill for a
   *  `not null` added to a table that already has rows. */
  ddl: string;
}

/** A field being added to an entity. */
export interface JpaAttributeSpec {
  name: string;
  /** The field's type, or — for a relation — the entity on the other end. */
  type_text: string;
  /** How the value is mapped. A different axis from `relation`: a relation points at another
   *  entity, these four are ways of storing a value. */
  kind: JpaAttributeKind;
  column: string;
  /** `nullable = false` is written when this is off. */
  optional: boolean;
  unique: boolean;
  length: number | null;
  /** A field initializer, written as given (`0`, `new BigDecimal("0")`). */
  default_value: string;
  /** Bean Validation constraints, unqualified (`NotNull`, `Size`, `Email`). */
  validation: string[];
  /** `''` for a plain column, else `ManyToOne` / `OneToMany` / `ManyToMany` / `OneToOne`. */
  relation: string;
  /** How a to-many relation is held: `Set` (default), `List` or `Map`. */
  collection: string;
  /** The owning side's field name, for an inverse relation. */
  mapped_by: string;
  lazy: boolean;
  /** `cascade = {…}` members, unqualified. */
  cascade: string[];
  /** `orphanRemoval = true` — a child removed from the collection is deleted. Only meaningful on
   *  the side that owns the children. */
  orphan_removal: boolean;
  accessors: boolean;
}

/** How a non-relation attribute is mapped. */
export type JpaAttributeKind = 'base' | 'enum' | 'embedded' | 'lob';

/** The cascade types a relation form offers, in the order they are worth reading. */
export const JPA_CASCADE_TYPES = ['ALL', 'PERSIST', 'MERGE', 'REMOVE', 'REFRESH', 'DETACH'];

/** `[constraint, what it does]` — the Bean Validation constraints the attribute form offers. */
export const JPA_VALIDATIONS: [string, string][] = [
  ['NotNull', 'rejected before it reaches the database'],
  ['NotBlank', 'not null, and not only whitespace'],
  ['Size', 'kept in step with the column length'],
  ['Email', 'address format'],
  ['Positive', 'greater than zero'],
  ['PastOrPresent', 'not in the future'],
];

/** A `@Modifying` bulk write being added to a repository. */
export interface JpaModifySpec {
  /** A hand-written name instead of the derived one. */
  name: string;
  /** `true` for a delete, `false` for an update. */
  delete: boolean;
  /** Property paths the update assigns. Ignored for a delete. */
  assignments: string[];
  conditions: JpaCondition[];
  /** Return the number of rows affected rather than `void`. */
  returns_count: boolean;
}

export interface JpaGenerateRequest {
  root: string;
  kind:
    | 'repository'
    | 'projection'
    | 'query-method'
    | 'attribute'
    | 'named-query'
    | 'lifecycle'
    | 'modify-method';
  /** Fully-qualified or simple entity name. */
  entity: string;
  base?: string;
  /** The name being given: the projection interface, the named query, the callback method. */
  name?: string;
  fields?: string[];
  /** The repository to write into — required for a query or modify method, optional for a
   *  nested projection. */
  repository?: string;
  query?: JpaQuerySpec;
  attribute?: JpaAttributeSpec;
  modify?: JpaModifySpec;
  /** For `lifecycle`: which callback. */
  event?: string;
  /** For `named-query`: the JPQL. Empty gets a skeleton. */
  text?: string;
  source_root?: string;
  /** The buffer a file is open in, as `[path, live text]`. Used instead of what is on disk when
   *  the insertion targets that path — an offset from a stale disk copy lands somewhere else
   *  entirely in a buffer with unsaved edits. */
  open?: [string, string];
}

/** The entities, their addressable properties, the existing repositories, and the vocabulary the
 *  builder offers. Wire: `bennu_jpa_form_model`. */
export function jpaFormModel(root: string): Promise<JpaFormModel> {
  return bennu('bennu_jpa_form_model', { args: { root } });
}

/** Generate a repository, a projection or a query method. Returns text; writes nothing.
 *  Wire: `bennu_jpa_generate`. */
export function jpaGenerate(req: JpaGenerateRequest): Promise<JpaGenerated> {
  return bennu('bennu_jpa_generate', { args: req });
}
