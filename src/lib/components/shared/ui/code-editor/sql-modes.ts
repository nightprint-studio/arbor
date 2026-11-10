/**
 * SQL highlighting, per dialect — the CodeMirror languages, and nothing else.
 *
 * Lives in `code-editor/` because it is exactly what this folder is for: language data with no
 * Arbor concept in it. Picus binds the languages to a connection's dialect and layers its
 * schema-aware intelligence on top; Bennu highlights the `.sql` files that sit in a Java project's
 * resources and has no database at all. Both — and Bennu's SQL inside Java strings — ride on the one
 * lexer in `sql-lexer.ts`, so a rule is fixed once.
 *
 * The dialects are not interchangeable. They disagree about string escapes (PostgreSQL's `E'…'`,
 * Oracle's `q'[…]'`, and no backslash escape at all in a plain literal), dollar quoting and comment
 * nesting, and a preview that colours `DO $$ … $$` as one broken string reads as an error that isn't
 * there. The stock `@codemirror/legacy-modes` SQL modes this file used to wrap got the Oracle
 * backslash wrong (`'C:\temp\'` coloured the rest of the file as one string) and had no notion of
 * types, calls, parameters or a name after a dot.
 */

import { StreamLanguage } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { sqlStream } from './sql-lexer';

/**
 * Which SQL a buffer is written in.
 *
 * `portable` is a real answer and not "unknown": a script that must run on both engines is written
 * in their intersection, and it is the correct label for a file nobody has classified — better than
 * silently picking one engine's rules for a file that is specifically not written in them.
 */
export type SqlDialect = 'oracle' | 'postgres' | 'portable';

/**
 * Built once per dialect and shared: `CodeEditor` builds its extensions from the descriptor at
 * mount, so handing out a freshly-allocated `StreamLanguage` on every reactive read would churn the
 * editor for no reason.
 *
 * `portable` reads with both engines' vocabularies and with PostgreSQL's string rules, which are the
 * ones that invent nothing: `q'[…]'` is accepted too, because it cannot appear in a file that has to
 * run on PostgreSQL as well — and where it does, it is meant.
 */
const MODES: Record<SqlDialect, StreamLanguage<unknown>> = {
  oracle: StreamLanguage.define(sqlStream('oracle')) as StreamLanguage<unknown>,
  postgres: StreamLanguage.define(sqlStream('postgres')) as StreamLanguage<unknown>,
  portable: StreamLanguage.define(sqlStream('portable')) as StreamLanguage<unknown>,
};

/**
 * The CodeMirror language extension for a SQL dialect, for a
 * {@link import('./types').LanguageDescriptor}'s `cmExtension`.
 *
 * An unrecognised (or absent) dialect resolves to `portable`, never to a specific engine: falling
 * through to one silently is how a whole folder of portable scripts ends up highlighted by the
 * dialect it is written to avoid.
 */
export function sqlHighlight(dialect: SqlDialect | null | undefined): Extension {
  return sqlLanguage(dialect);
}

/**
 * The same mode as a {@link Language} rather than as an opaque `Extension` — for a caller that has to
 * *wrap* it rather than install it, which today means a ```sql fence inside a Markdown document
 * (`markdown()` takes `LanguageDescription`s, and one of those is built around a `LanguageSupport`
 * around a `Language`).
 *
 * Same objects as {@link sqlHighlight}, deliberately: two entry points onto one table, so the dialect
 * rules above cannot come out different in a fence than in a `.sql` file.
 */
export function sqlLanguage(dialect: SqlDialect | null | undefined): StreamLanguage<unknown> {
  return MODES[dialect ?? 'portable'] ?? MODES.portable;
}
