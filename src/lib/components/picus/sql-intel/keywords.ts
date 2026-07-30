/**
 * Keyword vocabularies, per dialect.
 *
 * Two jobs, and they pull in opposite directions:
 *
 *  • **Completion** wants the words worth offering — the ones you actually type.
 *  • **Diagnostics** want the words that are certainly *not* column references, so
 *    an unqualified-column check never mistakes `CURRENT_TIMESTAMP` for a column
 *    that happens to exist in two joined tables.
 *
 * The second job is why {@link RESERVED} is deliberately over-generous: a keyword
 * missing from the list can produce a false diagnostic, whereas a keyword too many
 * only means one candidate fewer in a popup. When in doubt, add the word.
 *
 * The dialect is always passed in — Picus has no "current dialect" anywhere (§1 of
 * the design), and a PL/pgSQL keyword offered inside an Oracle script would be a
 * suggestion that cannot compile.
 */

import type { Dialect } from '$lib/types/picus';

/** Statement openers, clause keywords and operators — common to both dialects. */
const CORE = [
  'SELECT', 'FROM', 'WHERE', 'GROUP BY', 'ORDER BY', 'HAVING', 'DISTINCT', 'AS',
  'INNER JOIN', 'LEFT JOIN', 'RIGHT JOIN', 'FULL JOIN', 'CROSS JOIN', 'JOIN', 'ON', 'USING',
  'UNION', 'UNION ALL', 'INTERSECT', 'EXISTS', 'NOT EXISTS', 'IN', 'NOT IN', 'BETWEEN',
  'LIKE', 'IS NULL', 'IS NOT NULL', 'AND', 'OR', 'NOT', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END',
  'INSERT INTO', 'VALUES', 'UPDATE', 'SET', 'DELETE FROM', 'MERGE INTO', 'TRUNCATE TABLE',
  'CREATE TABLE', 'CREATE OR REPLACE VIEW', 'CREATE INDEX', 'ALTER TABLE', 'DROP TABLE',
  'ADD COLUMN', 'PRIMARY KEY', 'FOREIGN KEY', 'REFERENCES', 'UNIQUE', 'CHECK', 'DEFAULT',
  'NOT NULL', 'CONSTRAINT', 'WITH', 'COMMIT', 'ROLLBACK', 'SAVEPOINT', 'GRANT', 'REVOKE',
  'COUNT(*)', 'SUM', 'AVG', 'MIN', 'MAX', 'COALESCE', 'CAST', 'ASC', 'DESC', 'ALL', 'ANY',
];

const ORACLE_ONLY = [
  'MINUS', 'CONNECT BY', 'START WITH', 'ROWNUM', 'SYSDATE', 'DUAL', 'NVL', 'NVL2', 'DECODE',
  'TO_DATE', 'TO_CHAR', 'TO_NUMBER', 'TRUNC', 'SUBSTR', 'INSTR', 'SEQUENCE', 'NEXTVAL', 'CURRVAL',
  'CREATE OR REPLACE PROCEDURE', 'CREATE OR REPLACE FUNCTION', 'CREATE OR REPLACE PACKAGE',
  'CREATE OR REPLACE TRIGGER', 'DECLARE', 'BEGIN', 'EXCEPTION', 'IS', 'LOOP', 'END LOOP',
  'FOR EACH ROW', 'RAISE_APPLICATION_ERROR', 'VARCHAR2', 'NUMBER', 'CLOB', 'BLOB', 'DATE',
  'TIMESTAMP', 'MERGE', 'PARTITION BY', 'OVER',
];

const POSTGRES_ONLY = [
  'EXCEPT', 'RETURNING', 'ON CONFLICT', 'ON CONFLICT DO NOTHING', 'DO UPDATE SET', 'LIMIT', 'OFFSET',
  'ILIKE', 'CURRENT_TIMESTAMP', 'NOW()', 'COALESCE', 'GENERATE_SERIES', 'NEXTVAL', 'CURRVAL', 'SETVAL',
  'CREATE OR REPLACE FUNCTION', 'CREATE OR REPLACE PROCEDURE', 'LANGUAGE PLPGSQL', 'DO $$',
  'DECLARE', 'BEGIN', 'EXCEPTION', 'LOOP', 'END LOOP', 'RAISE NOTICE', 'RAISE EXCEPTION',
  'TEXT', 'VARCHAR', 'NUMERIC', 'INTEGER', 'BIGINT', 'BOOLEAN', 'TIMESTAMPTZ', 'JSONB', 'SERIAL',
  'IF NOT EXISTS', 'IF EXISTS', 'PARTITION BY', 'OVER', 'MATERIALIZED',
];

/** The completion vocabulary for a dialect, sorted so the popup reads predictably. */
export function keywordsFor(dialect: Dialect): string[] {
  const extra = dialect === 'oracle' ? ORACLE_ONLY : POSTGRES_ONLY;
  return [...new Set([...CORE, ...extra])].sort();
}

// ── Functions ─────────────────────────────────────────────────────────────────
//
// Kept apart from the keywords because they are completed differently: a function
// is offered as `NAME()` with the caret landing **between** the parentheses, which
// is the whole reason it is worth offering at all. They are part of the vocabulary
// for the exclusion filter too — `COALESCE` is not a column, whatever the schema
// says.

/** Functions that take arguments. Offered as `NAME()`, caret inside. */
const CORE_FUNCTIONS = [
  'COUNT', 'SUM', 'AVG', 'MIN', 'MAX', 'COALESCE', 'CAST', 'UPPER', 'LOWER', 'TRIM',
  'LTRIM', 'RTRIM', 'LPAD', 'RPAD', 'REPLACE', 'ROUND', 'FLOOR', 'ABS', 'MOD',
  'GREATEST', 'LEAST', 'EXTRACT', 'ROW_NUMBER', 'RANK', 'DENSE_RANK', 'LAG', 'LEAD',
  'NULLIF',
];

const ORACLE_FUNCTIONS = [
  'NVL', 'NVL2', 'DECODE', 'TO_CHAR', 'TO_DATE', 'TO_NUMBER', 'TRUNC', 'SUBSTR',
  'INSTR', 'LENGTH', 'CEIL', 'ADD_MONTHS', 'MONTHS_BETWEEN', 'LAST_DAY', 'NEXT_DAY',
  'LISTAGG', 'REGEXP_LIKE', 'REGEXP_REPLACE', 'REGEXP_SUBSTR', 'INITCAP',
];

const POSTGRES_FUNCTIONS = [
  'NOW', 'DATE_TRUNC', 'TO_CHAR', 'TO_DATE', 'TO_NUMBER', 'TO_TIMESTAMP', 'AGE',
  'SUBSTRING', 'POSITION', 'LENGTH', 'CEIL', 'SPLIT_PART', 'STRING_AGG', 'ARRAY_AGG',
  'JSONB_BUILD_OBJECT', 'JSON_AGG', 'GENERATE_SERIES', 'NEXTVAL', 'CURRVAL', 'SETVAL',
  'REGEXP_REPLACE', 'REGEXP_MATCHES', 'INITCAP', 'CONCAT_WS',
];

/** Values written without parentheses — `SYSDATE`, not `SYSDATE()`. Getting this
 *  wrong is a syntax error on Oracle, which is why the two lists are separate. */
const CONSTANTS: Record<Dialect, string[]> = {
  oracle: ['SYSDATE', 'SYSTIMESTAMP', 'USER', 'NULL', 'ROWNUM'],
  postgres: ['CURRENT_DATE', 'CURRENT_TIMESTAMP', 'CURRENT_USER', 'LOCALTIMESTAMP', 'NULL'],
};

/** The callable functions for a dialect, sorted. */
export function functionsFor(dialect: Dialect): string[] {
  const extra = dialect === 'oracle' ? ORACLE_FUNCTIONS : POSTGRES_FUNCTIONS;
  return [...new Set([...CORE_FUNCTIONS, ...extra])].sort();
}

/** The parenthesis-free values for a dialect. */
export function constantsFor(dialect: Dialect): string[] {
  return CONSTANTS[dialect];
}

/**
 * Every single word that can appear in SQL without being a column reference.
 *
 * Derived from the completion lists (split on spaces, so `GROUP BY` contributes
 * both halves) plus the function names and noise words that never make it into a
 * popup. Used as an exclusion filter, never as a suggestion source.
 */
export const RESERVED: ReadonlySet<string> = new Set(
  [
    ...CORE, ...ORACLE_ONLY, ...POSTGRES_ONLY,
    ...CORE_FUNCTIONS, ...ORACLE_FUNCTIONS, ...POSTGRES_FUNCTIONS,
    ...CONSTANTS.oracle, ...CONSTANTS.postgres,
  ]
    .flatMap((k) => k.split(/[\s(*)$]+/))
    .filter(Boolean)
    .concat([
      'BY', 'INTO', 'TABLE', 'VIEW', 'INDEX', 'COLUMN', 'ROW', 'ROWS', 'ONLY', 'FETCH', 'NEXT',
      'FIRST', 'LAST', 'NULL', 'TRUE', 'FALSE', 'NULLS', 'RETURN', 'RETURNS', 'LANGUAGE',
      'REPLACE', 'CREATE', 'ALTER', 'DROP', 'RENAME', 'COMMENT', 'CALL', 'EXECUTE', 'IMMEDIATE',
      'CURSOR', 'OPEN', 'CLOSE', 'FETCH', 'IF', 'ELSIF', 'ELSEIF', 'WHILE', 'EXIT', 'CONTINUE',
      'RAISE', 'PERFORM', 'STRICT', 'FOUND', 'RECORD', 'TYPE', 'ROWTYPE', 'CONSTANT', 'OUT', 'INOUT',
      'LOWER', 'UPPER', 'TRIM', 'LTRIM', 'RTRIM', 'LENGTH', 'REPLACE', 'ROUND', 'FLOOR', 'CEIL',
      'ABS', 'MOD', 'GREATEST', 'LEAST', 'EXTRACT', 'INTERVAL', 'CURRENT_DATE', 'CURRENT_USER',
      'LOCALTIMESTAMP', 'SYSTIMESTAMP', 'ROW_NUMBER', 'RANK', 'DENSE_RANK', 'LAG', 'LEAD',
      'STRING_AGG', 'LISTAGG', 'ARRAY', 'UNNEST', 'LATERAL', 'NATURAL', 'OUTER', 'LEFT', 'RIGHT',
      'FULL', 'INNER', 'CROSS', 'TEMPORARY', 'TEMP', 'CASCADE', 'RESTRICT', 'DEFERRABLE',
      'BEFORE', 'AFTER', 'INSTEAD', 'EACH', 'STATEMENT', 'ENABLE', 'DISABLE', 'TRIGGER',
      'SESSION', 'TRANSACTION', 'ISOLATION', 'LEVEL', 'READ', 'WRITE', 'ONLY', 'WORK',
    ])
    .map((k) => k.toUpperCase()),
);

/**
 * Words that terminate a table reference — the guard that stops alias reading from
 * swallowing the next clause. `FROM CLIENTI WHERE …` must not read `WHERE` as the
 * alias of `CLIENTI`, and that mistake would poison every feature at once.
 */
export const ALIAS_STOP: ReadonlySet<string> = new Set([
  'ON', 'USING', 'WHERE', 'GROUP', 'ORDER', 'HAVING', 'LIMIT', 'OFFSET', 'FETCH', 'UNION',
  'INTERSECT', 'EXCEPT', 'MINUS', 'JOIN', 'INNER', 'LEFT', 'RIGHT', 'FULL', 'CROSS', 'NATURAL',
  'LATERAL', 'SET', 'VALUES', 'RETURNING', 'WHEN', 'AND', 'OR', 'NOT', 'START', 'CONNECT',
  'WITH', 'FOR', 'WINDOW', 'INTO', 'PARTITION', 'TABLESAMPLE', 'AS', 'SELECT', 'FROM', 'DELETE',
  'UPDATE', 'INSERT', 'MERGE', 'BY', 'IS', 'IN', 'LIKE', 'BETWEEN', 'EXISTS', 'ORDERED',
]);

/** Statement openers that write. Drives the read-only diagnostic; the server is
 *  still the authority, this only makes the refusal arrive before you press Run. */
export const WRITE_STARTERS: ReadonlySet<string> = new Set([
  'INSERT', 'UPDATE', 'DELETE', 'MERGE', 'TRUNCATE', 'CREATE', 'ALTER', 'DROP', 'RENAME',
  'GRANT', 'REVOKE', 'COMMENT', 'CALL', 'DO', 'REFRESH', 'VACUUM', 'ANALYZE', 'REINDEX',
  'COPY', 'LOCK', 'SET',
]);

/** Statement openers whose body is procedural — a block, not a statement. Those are
 *  excluded from schema diagnostics: a scanner is at its weakest inside a block and
 *  a wrong warning there would be the reason someone turns the feature off. */
export const BLOCK_STARTERS: ReadonlySet<string> = new Set([
  'DECLARE', 'BEGIN', 'DO', 'END', 'EXCEPTION', 'IF', 'ELSIF', 'ELSEIF', 'ELSE', 'LOOP',
  'WHILE', 'FOR', 'RETURN', 'RAISE', 'PERFORM', 'EXIT', 'OPEN', 'CLOSE', 'FETCH', 'COMMIT',
  'ROLLBACK', 'SAVEPOINT', 'EXECUTE',
]);
