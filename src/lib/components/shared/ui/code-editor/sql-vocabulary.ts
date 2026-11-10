/**
 * The words of SQL, per dialect — data only, consumed by {@link import('./sql-lexer')}.
 *
 * Kept apart from the lexer because a vocabulary is edited by adding a word and a lexer by changing
 * a rule; mixing them makes every word addition a review of the tokenizer.
 *
 * The lists are deliberately *not* exhaustive "reserved word" tables. A highlighter that colours every
 * reserved word of the standard turns `name`, `value`, `type`, `user` and `level` — all legal column
 * names — into keywords, which reads as a mistake. What is listed is what a person reading a script
 * expects to see coloured.
 */

const words = (list: string): ReadonlySet<string> => new Set(list.split(/\s+/).filter(Boolean));

/** Words that can directly follow `from` / `join` / `into` / … and are still the language, not a name:
 *  `from (select …)`, `join lateral`, `drop table if exists`, `on conflict do update set`. Anything else in that
 *  position is a name the project chose — which is how a table called `order` stays a table. */
export const RESERVED = words(`
  select with values set lateral only if not exists temporary temp unlogged materialized recursive
  table view global local
`);

/** The subset of {@link RESERVED} that modifies the introducer rather than ending it: after `if exists` the
 *  name is still to come. */
export const NAME_MODIFIERS = words('lateral only if not exists temporary temp unlogged materialized recursive table view global local');

/** Keywords of the SQL every engine shares. */
export const COMMON_KEYWORDS = words(`
  add all alter and any as asc between by case check column commit constraint create cross cascade
  default delete desc distinct drop else end except exists foreign from full group having in index
  inner insert intersect into is join key left like limit not offset on or order outer primary
  references restrict right rollback savepoint select set table then to truncate union unique update
  using values view when where with begin transaction grant revoke schema database sequence trigger
  procedure function return returns returning declare cursor fetch open close for each row rows only
  over partition range unbounded preceding following current window filter within nulls first last
  escape natural recursive temporary temp replace if exists cascade rename comment explain analyze
  lateral exception language
`);

/** What PostgreSQL adds. */
export const POSTGRES_KEYWORDS = words(`
  ilike similar analyse vacuum cluster reindex copy do perform raise notice elsif elseif loop while
  exit continue foreach slice plpgsql security definer invoker volatile stable immutable strict
  parallel safe unsafe cost inherits tablespace owner extension domain collate collation concurrently
  conflict nothing materialized unlogged only verbose truncate listen notify unlisten stdin stdout
  program header csv delimiter force quote encoding generated always identity stored virtual
  overriding tablesample bernoulli ordinality
`);

/** What Oracle SQL / PL/SQL adds. */
export const ORACLE_KEYWORDS = words(`
  connect prior start nocycle siblings merge matched pragma bulk collect forall elsif exception
  package body authid definer current_user pipelined deterministic result_cache nocopy out inout
  type subtype record cursor loop while exit continue goto raise_application_error pls_integer
  binary_integer rowtype synonym public database link tablespace storage pctfree initrans
  logging nologging parallel noparallel compress nocompress online enable disable validate
  novalidate rely deferrable initially deferred immediate varray nested cube rollup grouping sets
  pivot unpivot model dimension measures rules iterate keep dense_rank sample seed flashback
  versions scn timestamp minvalue maxvalue nominvalue nomaxvalue cycle nocycle order noorder cache
  nocache identified account unlock lock quota profile directory
`);

/** What JPQL / HQL adds (and, with {@link COMMON_KEYWORDS}, all it uses). */
export const JPQL_KEYWORDS = words(`
  select from where order by group having join left right inner outer fetch distinct new as and or
  not in like between is null empty member of exists all any some asc desc update set delete insert
  into values case when then else end on with escape treat
  lateral cross full nulls first last offset limit union intersect except
`);

/** Keywords that are also called like functions: `left(s, 2)`, `replace(a, b, c)`. */
export const CALLABLE_KEYWORDS = words('left right replace insert truncate');

/** Data types. */
export const COMMON_TYPES = words(`
  int integer smallint bigint tinyint decimal numeric real float double precision boolean bool char
  character varchar nchar nvarchar text clob blob bytea date time timestamp timestamptz interval
  binary varbinary bit varying uuid json jsonb xml serial bigserial smallserial money
`);

export const POSTGRES_TYPES = words(`
  int2 int4 int8 float4 float8 bpchar citext inet cidr macaddr macaddr8 tsvector tsquery oid regclass
  regtype regproc point line lseg box path polygon circle int4range int8range numrange tsrange
  tstzrange daterange hstore ltree record void anyelement anyarray
`);

export const ORACLE_TYPES = words(`
  number varchar2 nvarchar2 nclob long raw rowid urowid bfile binary_float binary_double
  binary_integer pls_integer natural naturaln positive positiven simple_integer sys_refcursor
  xmltype anydata timestamp
`);

/** Types that are also function names: `date(x)`, `char(65)`, `timestamp(3)` after a name is a type. */
export const AMBIGUOUS_TYPES = words('char character date time timestamp interval left right bit');

/** Built-in functions. Coloured as calls only when followed by `(` — the same word may be a column. */
export const COMMON_FUNCTIONS = words(`
  abs avg ceil ceiling coalesce concat count exp floor greatest least length lower ltrim max min mod
  nullif power round rtrim sign sqrt substr substring sum trim upper cast extract position
  row_number rank dense_rank ntile lag lead first_value last_value nth_value percent_rank cume_dist
  array_agg string_agg listagg json_agg jsonb_agg json_object json_array xmlagg xmlelement
  to_char to_date to_number to_timestamp regexp_replace regexp_like regexp_substr regexp_instr
  regexp_matches date_trunc date_part age now random setval nextval currval generate_series unnest
  array_length jsonb_build_object json_build_object jsonb_set jsonb_extract_path
  nvl nvl2 decode add_months months_between last_day next_day trunc instr lpad rpad replace translate
  sysdate systimestamp sys_guid dbms_output
`);

/** Constants and pseudo-columns: coloured without a call. */
export const ATOMS = words(`
  true false null unknown current_date current_time current_timestamp localtime localtimestamp
  current_user session_user sysdate systimestamp rownum rowid
`);

/** JPQL operators-as-words that read as keywords inside a function-like call (`size(o.items)`). */
export const JPQL_FUNCTIONS = words(`
  count sum avg min max abs sqrt mod size length locate lower upper trim substring concat coalesce
  nullif current_date current_time current_timestamp function index key value entry type cast
  extract round ceiling floor exp ln power sign left right replace year month day hour minute second
  local_date local_time local_datetime
`);

export type SqlVocabularyDialect = 'oracle' | 'postgres' | 'portable' | 'jpql';

export interface SqlVocabulary {
  keywords: ReadonlySet<string>;
  types: ReadonlySet<string>;
  functions: ReadonlySet<string>;
}

const merge = (...sets: ReadonlySet<string>[]): ReadonlySet<string> => new Set(sets.flatMap((s) => [...s]));

const VOCABULARIES: Record<SqlVocabularyDialect, SqlVocabulary> = {
  postgres: {
    keywords: merge(COMMON_KEYWORDS, POSTGRES_KEYWORDS),
    types: merge(COMMON_TYPES, POSTGRES_TYPES),
    functions: COMMON_FUNCTIONS,
  },
  oracle: {
    keywords: merge(COMMON_KEYWORDS, ORACLE_KEYWORDS),
    types: merge(COMMON_TYPES, ORACLE_TYPES),
    functions: COMMON_FUNCTIONS,
  },
  // Portable is the union: a script written to run on both reads with both engines' words, since it
  // may legitimately use either's spelling inside a branch the other never executes.
  portable: {
    keywords: merge(COMMON_KEYWORDS, POSTGRES_KEYWORDS, ORACLE_KEYWORDS),
    types: merge(COMMON_TYPES, POSTGRES_TYPES, ORACLE_TYPES),
    functions: COMMON_FUNCTIONS,
  },
  jpql: {
    keywords: JPQL_KEYWORDS,
    types: new Set(),
    functions: JPQL_FUNCTIONS,
  },
};

export function vocabularyOf(dialect: SqlVocabularyDialect): SqlVocabulary {
  return VOCABULARIES[dialect];
}
