/**
 * SQL highlighting, per dialect — the CodeMirror stream modes, and nothing else.
 *
 * Lives in `code-editor/` because it is exactly what this folder is for: language
 * data with no Arbor concept in it. Two products need the same three modes for
 * different reasons — Picus binds them to a connection's dialect and layers its
 * schema-aware intelligence on top; Bennu highlights the `.sql` files that sit in a
 * Java project's resources and has no database at all — and a copy in each would
 * mean the Oracle fix below getting fixed once.
 *
 * The dialects are not interchangeable keyword lists. They disagree about string
 * escapes, dollar quoting and comment forms, and a preview that colours `DO $$ … $$`
 * as one broken string reads as an error that isn't there.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { sql, pgSQL } from '@codemirror/legacy-modes/mode/sql';
import type { Extension } from '@codemirror/state';

/**
 * Which SQL a buffer is written in.
 *
 * `portable` is a real answer and not "unknown": a script that must run on both
 * engines is written in their intersection, and it is the correct label for a file
 * nobody has classified — better than silently picking one engine's rules for a file
 * that is specifically not written in them.
 */
export type SqlDialect = 'oracle' | 'postgres' | 'portable';

/** The mode's own "space-separated list → lookup" helper, which it does not export. */
function set(words: string): Record<string, boolean> {
  return Object.fromEntries(words.split(' ').map((word) => [word, true]));
}

/**
 * Oracle, with the one thing the stock mode gets wrong put right.
 *
 * `plSQL` leaves `backslashStringEscapes` unset, and the mode's default is **true** —
 * so a backslash inside a literal is read as escaping the next character. Oracle has
 * no such rule: `'C:\temp\'` is a complete string, and the stock mode sees the final
 * `\'` as an escaped quote, decides the literal is still open, and colours the **rest
 * of the file** as one string. Every path, every regex, every Windows directory in a
 * script does it.
 *
 * Rebuilt through the exported `sql()` factory rather than patched, because the config
 * is the mode's only seam: the object `sql()` returns is a tokenizer and does **not**
 * carry its own `parserConfig`, so spreading it would silently give the generic SQL
 * keyword list instead of Oracle's — the kind of downgrade that looks like it worked.
 *
 * The word lists below are `plSQL`'s own, copied from
 * `@codemirror/legacy-modes/mode/sql.js` at the version in `package.json`. They are
 * data, not logic; if the upstream mode ever gains a `backslashStringEscapes` of its
 * own, this whole constant becomes `plSQL` again.
 */
const oracleMode: StreamParser<unknown> = sql({
  client: set(
    'appinfo arraysize autocommit autoprint autorecovery autotrace blockterminator break btitle cmdsep colsep compatibility compute concat copycommit copytypecheck define describe echo editfile embedded escape exec execute feedback flagger flush heading headsep instance linesize lno loboffset logsource long longchunksize markup native newpage numformat numwidth pagesize pause pno recsep recsepchar release repfooter repheader serveroutput shiftinout show showmode size spool sqlblanklines sqlcase sqlcode sqlcontinue sqlnumber sqlpluscompatibility sqlprefix sqlprompt sqlterminator suffix tab term termout time timing trimout trimspool ttitle underline verify version wrap',
  ),
  keywords: set(
    'abort accept access add all alter and any array arraylen as asc assert assign at attributes audit authorization avg base_table begin between binary_integer body boolean by case cast char char_base check close cluster clusters colauth column comment commit compress connect connected constant constraint crash create current currval cursor data_base database date dba deallocate debugoff debugon decimal declare default definition delay delete desc digits dispose distinct do drop else elseif elsif enable end entry escape exception exception_init exchange exclusive exists exit external fast fetch file for force form from function generic goto grant group having identified if immediate in increment index indexes indicator initial initrans insert interface intersect into is key level library like limited local lock log logging long loop master maxextents maxtrans member minextents minus mislabel mode modify multiset new next no noaudit nocompress nologging noparallel not nowait number_base object of off offline on online only open option or order out package parallel partition pctfree pctincrease pctused pls_integer positive positiven pragma primary prior private privileges procedure public raise range raw read rebuild record ref references refresh release rename replace resource restrict return returning returns reverse revoke rollback row rowid rowlabel rownum rows run savepoint schema segment select separate session set share snapshot some space split sql start statement storage subtype successful synonym tabauth table tables tablespace task terminate then to trigger truncate type union unique unlimited unrecoverable unusable update use using validate value values variable view views when whenever where while with work',
  ),
  builtin: set(
    'abs acos add_months ascii asin atan atan2 average bfile bfilename bigserial bit blob ceil character chartorowid chr clob concat convert cos cosh count dec decode deref dual dump dup_val_on_index empty error exp false float floor found glb greatest hextoraw initcap instr instrb int integer isopen last_day least length lengthb ln lower lpad ltrim lub make_ref max min mlslabel mod months_between natural naturaln nchar nclob new_time next_day nextval nls_charset_decl_len nls_charset_id nls_charset_name nls_initcap nls_lower nls_sort nls_upper nlssort no_data_found notfound null number numeric nvarchar2 nvl others power rawtohex real reftohex round rowcount rowidtochar rowtype rpad rtrim serial sign signtype sin sinh smallint soundex sqlcode sqlerrm sqrt stddev string substr substrb sum sysdate tan tanh to_char text to_date to_label to_multi_byte to_number to_single_byte translate true trunc uid unlogged upper user userenv varchar varchar2 variance varying vsize xml',
  ),
  operatorChars: /^[*/+\-%<>!=~]/,
  dateSQL: set('date time timestamp'),
  support: set('doubleQuote nCharCast zerolessFloat binaryNumber hexNumber'),
  // The one line that is ours. Oracle has no backslash escape in a string literal;
  // the mode's default says it does.
  backslashStringEscapes: false,
});

/**
 * Built once per dialect and shared: `CodeEditor` builds its extensions from the
 * descriptor at mount, so handing out a freshly-allocated `StreamLanguage` on every
 * reactive read would churn the editor for no reason.
 *
 * `portable` is PostgreSQL's mode, and that is not an arbitrary pick between two
 * wrongs. Of the two engines, `pgSQL` is the one whose string rules cover the
 * intersection without inventing anything: it already has `backslashStringEscapes:
 * false`, and the Oracle form it lacks (`q'[…]'`) cannot appear in a file that has to
 * run on PostgreSQL too.
 *
 * ## What neither mode does: `''`
 *
 * The upstream tokenizer closes a literal at the first unescaped quote, so
 * `'L''Aquila'` is scanned as **two adjacent strings** rather than one. Measured, not
 * assumed — this comment previously claimed the opposite.
 *
 * It is invisible, and deliberately left alone. Both halves carry the `string` tag,
 * so the colour runs unbroken, and the split always lands *on* a quote — which means
 * the tokenizer immediately re-enters a string and the pairing can never come out
 * odd. What follows the literal is highlighted correctly.
 *
 * The scanner in `picus/sql-intel/tokens.ts` does read `''` as one literal, because
 * there it decides where completion and diagnostics may act, and being off by one
 * literal there would be visible. Here it would buy nothing that can be seen.
 */
const MODES: Record<SqlDialect, Extension> = {
  oracle: StreamLanguage.define(oracleMode),
  postgres: StreamLanguage.define(pgSQL),
  portable: StreamLanguage.define(pgSQL),
};

/**
 * The CodeMirror language extension for a SQL dialect, for a
 * {@link import('./types').LanguageDescriptor}'s `cmExtension`.
 *
 * An unrecognised (or absent) dialect resolves to `portable`, never to a specific
 * engine: falling through to one silently is how a whole folder of portable scripts
 * ends up highlighted by the dialect it is written to avoid.
 */
export function sqlHighlight(dialect: SqlDialect | null | undefined): Extension {
  return MODES[dialect ?? 'portable'] ?? MODES.portable;
}
