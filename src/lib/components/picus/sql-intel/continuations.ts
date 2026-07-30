/**
 * What may come next — the grammar half of completion.
 *
 * Completion used to answer one question ("what names exist?") and append the
 * keyword list to the end of it. That is wrong in both directions at once, and
 * both were visible on the first keystroke:
 *
 *  • **Names where no name can go.** An empty statement was answered with every
 *    table in the database. No SQL statement begins with a table name — the popup
 *    was wrong 100% of the time in the one position you are in most often.
 *  • **Keywords nowhere.** They were pushed last and then cut by the 500-option
 *    ceiling, so on a schema of any size the keyword half of the vocabulary simply
 *    never appeared. Typing `SEL` in a real database offered no `SELECT`.
 *
 * So the caret's position decides an {@link Expectation} first — which *kind* of
 * name belongs here, which keywords, and which of the two ranks higher — and the
 * candidates are built to fit it. The rule stays the product's rule: every
 * proposal is a fact (a catalogue name, or a word the grammar accepts here), never
 * a guess.
 *
 * ## Exclusivity
 *
 * Some positions have one answer. After `INSERT` the language accepts `INTO` and
 * nothing else; after `GROUP`, `BY`. Those are marked **exclusive** and suppress
 * the schema entirely — a popup of 900 tables where one word is legal is not a
 * smaller help, it is a wrong one.
 */

import type { Dialect } from '$lib/types/picus';
import type { Clause, StatementInfo } from './analysis';
import { constantsFor, functionsFor, keywordsFor } from './keywords';

/** Which family of names belongs at the caret. */
export type NameKind =
  /** Nothing from the catalogue — the grammar wants a keyword or a new name. */
  | 'none'
  /** Tables, views and the statement's CTEs. */
  | 'relations'
  /** Columns of the relations in scope, plus their aliases. */
  | 'columns'
  /** Columns of the statement's write target only. */
  | 'target-columns'
  /** Literals' company: sequences and functions, never columns. */
  | 'values'
  /** Mid-statement with no clause read yet — offer both, columns first. */
  | 'any';

export interface Expectation {
  names: NameKind;
  /** Keywords worth offering here, already in preference order. */
  keywords: string[];
  /** Functions and parenthesis-free constants belong here too. */
  functions: boolean;
  /** Offer the keywords and nothing else. */
  exclusive: boolean;
  /** Keywords outrank names — the caret sits between clauses rather than inside one. */
  keywordsFirst: boolean;
}

// ── Statement openers ─────────────────────────────────────────────────────────

const CORE_OPENERS = [
  'SELECT', 'INSERT INTO', 'UPDATE', 'DELETE FROM', 'MERGE INTO', 'WITH',
  'CREATE TABLE', 'CREATE OR REPLACE VIEW', 'CREATE INDEX', 'CREATE SEQUENCE',
  'ALTER TABLE', 'DROP TABLE', 'TRUNCATE TABLE', 'COMMENT ON',
  'GRANT', 'REVOKE', 'COMMIT', 'ROLLBACK', 'SAVEPOINT',
];

const OPENERS: Record<Dialect, string[]> = {
  oracle: [
    ...CORE_OPENERS,
    'CREATE OR REPLACE PROCEDURE', 'CREATE OR REPLACE FUNCTION',
    'CREATE OR REPLACE PACKAGE', 'CREATE OR REPLACE TRIGGER',
    'DECLARE', 'BEGIN', 'ALTER SESSION SET',
  ],
  postgres: [
    ...CORE_OPENERS,
    'CREATE OR REPLACE FUNCTION', 'CREATE OR REPLACE PROCEDURE', 'CREATE TRIGGER',
    'CREATE MATERIALIZED VIEW', 'REFRESH MATERIALIZED VIEW',
    'DO $$', 'DECLARE', 'BEGIN', 'ANALYZE', 'VACUUM', 'SET',
  ],
};

// ── One word decides the next ─────────────────────────────────────────────────

/**
 * Positions where the grammar leaves no choice.
 *
 * Deliberately short. A word belongs here only when *every* legal continuation is
 * in the list — `ON` is absent because a join predicate starts with a column, and
 * `SET` is absent for the same reason. A near-miss here removes real candidates,
 * which is a worse failure than one extra keyword in a long popup.
 */
const NEXT_WORD: Record<string, string[]> = {
  INSERT: ['INTO'],
  DELETE: ['FROM'],
  GROUP: ['BY'],
  ORDER: ['BY'],
  PARTITION: ['BY'],
  IS: ['NULL', 'NOT NULL'],
  LEFT: ['JOIN', 'OUTER JOIN'],
  RIGHT: ['JOIN', 'OUTER JOIN'],
  FULL: ['JOIN', 'OUTER JOIN'],
  INNER: ['JOIN'],
  CROSS: ['JOIN'],
  OUTER: ['JOIN'],
  UNION: ['ALL'],
  CREATE: [
    'TABLE', 'OR REPLACE VIEW', 'INDEX', 'UNIQUE INDEX', 'SEQUENCE',
    'OR REPLACE FUNCTION', 'OR REPLACE PROCEDURE', 'OR REPLACE TRIGGER',
  ],
  ALTER: ['TABLE', 'SEQUENCE', 'INDEX'],
  DROP: ['TABLE', 'VIEW', 'INDEX', 'SEQUENCE', 'FUNCTION', 'PROCEDURE', 'TRIGGER'],
  TRUNCATE: ['TABLE'],
  FETCH: ['FIRST', 'NEXT'],
  MERGE: ['INTO'],
  COMMENT: ['ON'],
};

/** `NOT` is answered per dialect — `ILIKE` exists on one engine only. */
function afterNot(dialect: Dialect): string[] {
  return dialect === 'postgres'
    ? ['NULL', 'IN', 'EXISTS', 'LIKE', 'ILIKE', 'BETWEEN']
    : ['NULL', 'IN', 'EXISTS', 'LIKE', 'BETWEEN'];
}

// ── Clause vocabularies ───────────────────────────────────────────────────────

/** What can follow a completed table reference in a `FROM`. */
function afterRelation(dialect: Dialect): string[] {
  const core = [
    'WHERE', 'INNER JOIN', 'LEFT JOIN', 'RIGHT JOIN', 'FULL JOIN', 'CROSS JOIN', 'JOIN',
    'ON', 'USING', 'GROUP BY', 'ORDER BY', 'HAVING', 'UNION', 'UNION ALL', 'AS',
  ];
  return dialect === 'postgres'
    ? [...core, 'LIMIT', 'OFFSET', 'EXCEPT', 'INTERSECT']
    : [...core, 'MINUS', 'INTERSECT', 'CONNECT BY', 'START WITH'];
}

/** What can follow a completed operand inside a predicate. Word-shaped only —
 *  nobody reaches for a popup to type `=`. */
function afterOperand(dialect: Dialect): string[] {
  const core = [
    'IS NULL', 'IS NOT NULL', 'IN', 'NOT IN', 'LIKE', 'NOT LIKE', 'BETWEEN',
    'AND', 'OR', 'NOT', 'EXISTS', 'NOT EXISTS',
  ];
  return dialect === 'postgres' ? [...core, 'ILIKE'] : core;
}

const SELECT_WORDS = ['DISTINCT', 'ALL', 'AS', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END', 'FROM'];
const ORDER_WORDS = ['ASC', 'DESC', 'NULLS FIRST', 'NULLS LAST'];

// ── The decision ──────────────────────────────────────────────────────────────

export interface CaretFacts {
  clause: Clause;
  /** The word immediately before the one being typed, upper-cased. `''` when none. */
  previousWord: string;
  /** The statement has no token before the caret — it is still empty. */
  atStatementStart: boolean;
  /** The caret sits right after a completed table reference in a `FROM`. */
  afterRelationRef: boolean;
  /** The user has typed a prefix; with none, a keyword continuation is likelier
   *  than the start of a name. */
  hasPrefix: boolean;
  info: StatementInfo | null;
}

/**
 * What belongs at the caret.
 *
 * Pure, and deliberately readable as a table: this function is the policy, and a
 * policy that cannot be read in one screen is one nobody can correct.
 */
export function expectationAt(facts: CaretFacts, dialect: Dialect): Expectation {
  const { clause, previousWord, atStatementStart, afterRelationRef, hasPrefix } = facts;

  // Nothing typed yet: the only words that can start a statement.
  if (atStatementStart) {
    return {
      names: 'none', keywords: OPENERS[dialect], functions: false,
      exclusive: true, keywordsFirst: true,
    };
  }

  // One word decides the next — outside a block.
  //
  // Inside one the same words mean other things (`IS` opens a PL/SQL body,
  // `CREATE` may be a dynamic string) and suppressing the schema on a guess that
  // holds for statements would remove the only useful candidates. So a procedural
  // statement keeps the suggestion and drops the exclusivity.
  const decided = previousWord === 'NOT' ? afterNot(dialect) : NEXT_WORD[previousWord];
  if (decided) {
    const inBlock = !!facts.info?.procedural;
    return {
      names: inBlock ? 'any' : 'none', keywords: decided, functions: inBlock,
      exclusive: !inBlock, keywordsFirst: true,
    };
  }

  // A completed `FROM ORDINI o` — what follows is a clause, not another name.
  if (afterRelationRef && !hasPrefix) {
    return {
      names: 'relations', keywords: afterRelation(dialect), functions: false,
      exclusive: false, keywordsFirst: true,
    };
  }

  switch (clause) {
    case 'from':
      return {
        names: 'relations', keywords: afterRelationRef ? afterRelation(dialect) : [],
        functions: false, exclusive: false, keywordsFirst: false,
      };

    case 'select':
      return {
        names: 'columns', keywords: SELECT_WORDS, functions: true,
        exclusive: false, keywordsFirst: false,
      };

    case 'where':
    case 'on':
    case 'having':
      return {
        names: 'columns', keywords: afterOperand(dialect), functions: true,
        exclusive: false, keywordsFirst: !hasPrefix,
      };

    case 'set':
      return {
        names: 'target-columns', keywords: ['NULL', 'DEFAULT', 'CASE', 'WHERE'],
        functions: true, exclusive: false, keywordsFirst: false,
      };

    // The parenthesised list of an INSERT is the target's columns and nothing
    // else — a keyword in there does not parse.
    case 'insert-cols':
    case 'using-cols':
      return {
        names: clause === 'insert-cols' ? 'target-columns' : 'columns',
        keywords: [], functions: false, exclusive: false, keywordsFirst: false,
      };

    case 'values':
      return {
        names: 'values', keywords: ['NULL', 'DEFAULT'], functions: true,
        exclusive: false, keywordsFirst: false,
      };

    case 'group':
      return {
        names: 'columns', keywords: ['HAVING', 'ORDER BY'], functions: true,
        exclusive: false, keywordsFirst: false,
      };

    case 'order':
      return {
        names: 'columns', keywords: ORDER_WORDS, functions: false,
        exclusive: false, keywordsFirst: !hasPrefix,
      };

    case 'returning':
      return {
        names: 'target-columns', keywords: [], functions: false,
        exclusive: false, keywordsFirst: false,
      };

    default:
      return {
        names: 'any', keywords: keywordsFor(dialect), functions: true,
        exclusive: false, keywordsFirst: false,
      };
  }
}

export { functionsFor, constantsFor };
