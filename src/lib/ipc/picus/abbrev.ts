/**
 * The SQL abbreviation expander, over the wire.
 *
 * `s#localstrings(keycode,value)[keycode='ita']` becomes
 * `SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'`. The language is
 * `arbor-sql-abbrev`; the schema, the dialect and the emitter are Picus's, which is
 * why the expansion is worth having at all — the column's **type** decides where the
 * quotes go and the **foreign key** decides what a `>` join is `ON`, and neither of
 * those is a thing an editor snippet could know.
 *
 * ## One verb, three answers
 *
 * `picus_expand_sql` is asked on every keystroke in a buffer that is mostly ordinary
 * SQL, so it answers for ordinary SQL too — {@link Expansion.isAbbreviation} is
 * `false` and nothing else comes back. That is the property the whole editor
 * integration rests on: the caller never has to decide *first* whether the line is
 * an abbreviation, and so never grows a second, drifting copy of that rule.
 *
 * Expansion and cursor context come from the **same call**, deliberately. Two calls
 * could disagree about where the caret is, and then completion would offer the
 * columns of a table the expansion does not think is there.
 *
 * A refusal — `error` — is a first-class answer, not a failure: the language refuses
 * rather than guesses (an ambiguous foreign key, an unknown column, an `UPDATE`
 * Picus's generator cannot key), and those sentences are written for the person
 * typing. Swallowing one would turn "it told me why" into "it did nothing".
 */

import type { Dialect } from '$lib/types/picus';
import { picus } from '../rpc';

/**
 * Where the caret is inside an abbreviation, and therefore what is worth offering.
 *
 * The variants and their fields mirror `arbor_sql_abbrev::prelude::CursorContext`
 * one for one — it is an externally tagged-by-`at` enum with camelCase variant
 * names, and its struct fields are single words that need no renaming. Everything
 * here is text **as typed**: the backend consults no schema to answer this, so
 * `tables` are the names the user wrote and resolving them is this side's job.
 */
export type CursorContext =
  /** Before the `#`. The four verbs. */
  | { at: 'verb'; prefix: string }
  /** The root table. */
  | { at: 'table'; prefix: string }
  /** After a `>` — tables, ideally the ones `from` has a foreign key to or from. */
  | { at: 'joinTable'; from: string; prefix: string }
  /** After a `>table:` — the columns of the foreign keys between the two. */
  | { at: 'joinColumn'; from: string; to: string; prefix: string }
  /** A name inside `(...)`. */
  | { at: 'column'; tables: string[]; prefix: string }
  /** A value inside `(...)` — what an `UPDATE` sets a column to. */
  | { at: 'columnValue'; tables: string[]; column: string | null; prefix: string }
  /** A name inside `[...]`. */
  | { at: 'predicateColumn'; tables: string[]; prefix: string }
  /** Between a predicate's column and its value. */
  | { at: 'predicateOperator'; tables: string[]; column: string | null; prefix: string }
  /** A value inside `[...]`. */
  | { at: 'predicateValue'; tables: string[]; column: string | null; prefix: string }
  /** After `*`. */
  | { at: 'multiplier'; prefix: string }
  /** On punctuation, or past the end of the grammar. Nothing to offer. */
  | { at: 'none' };

/** What the editor gets back for the line the caret is on. */
export interface Expansion {
  /** Does this text even look like an abbreviation? The answer decides whether the
   *  editor shows anything at all — and it comes from the Rust parser, never from
   *  a shape test written here. */
  isAbbreviation: boolean;
  /** The SQL, when it expanded. */
  sql?: string;
  /** Why it did not, in the user's own words. */
  error?: string;
  /** What is under the caret, from the same parse as `sql`. */
  context: CursorContext;
}

/**
 * Expand what is being typed, and say what is under the caret.
 *
 * `cursor` is a **UTF-8 byte** offset into `input` — see `sql-intel/abbrev.ts`,
 * which owns the conversion from CodeMirror's UTF-16 coordinate. `dialect` decides
 * spelling only; a wrong one produces visibly wrong SQL in a preview, never a wrong
 * write.
 */
export function expandSql(
  id: string,
  input: string,
  cursor: number,
  dialect: Dialect,
): Promise<Expansion> {
  return picus('picus_expand_sql', { id, input, cursor, dialect });
}
