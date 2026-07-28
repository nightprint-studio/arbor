/**
 * SQL language descriptors for the shared CodeMirror host.
 *
 * Highlighting rides on CodeMirror's legacy SQL modes (already a dependency),
 * picked **per dialect**: `plSQL` for Oracle, `pgSQL` for PostgreSQL. That
 * matters beyond keyword lists — the two disagree about string escapes, dollar
 * quoting and comment forms, and a preview that colours `DO $$ … $$` as one
 * broken string reads as an error that isn't there.
 *
 * Intelligence — completion, hover and ghost text — comes from `sql-intel/`,
 * attached here through `intel`. With a `cmExtension` descriptor the *tree*-driven
 * hooks (`resolveGoto`, `foldNode`) are inactive because there is no live tree;
 * the `intel` hooks and the `diagnostics` prop do not care and work as they are.
 * Wiring the real `picus-parse` grammar into the editor is a separate, later
 * decision — and the one that unlocks in-buffer navigation.
 *
 * The real parse — the one the inventory, the analysis and the rewriter are built
 * on — belongs to `picus-parse` in the backend, and never to the editor.
 */

import { StreamLanguage } from '@codemirror/language';
import { sql, pgSQL } from '@codemirror/legacy-modes/mode/sql';
import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor/types';
import type { Dialect } from '$lib/types/picus';
import { createSqlIntel } from './sql-intel';

/** The tree-sitter half of the descriptor is unused for `cmExtension` languages. */
const NO_TREE = {
  createParser: () => Promise.reject(new Error('picus SQL highlights through CodeMirror, not tree-sitter')),
  classify: () => null,
} as const;

/**
 * Oracle, with the one thing the stock mode gets wrong put right.
 *
 * `plSQL` leaves `backslashStringEscapes` unset, and the mode's default is
 * **true** — so a backslash inside a literal is read as escaping the next
 * character. Oracle has no such rule: `'C:\temp\'` is a complete string, and the
 * stock mode sees the final `\'` as an escaped quote, decides the literal is
 * still open, and colours the **rest of the file** as one string. Every path,
 * every regex, every Windows directory in a script does it.
 *
 * Rebuilt through the exported `sql()` factory rather than patched, because the
 * config is the mode's only seam: the object `sql()` returns is a tokenizer and
 * does **not** carry its own `parserConfig`, so spreading it would silently give
 * the generic SQL keyword list instead of Oracle's — the kind of downgrade that
 * looks like it worked.
 *
 * The word lists below are `plSQL`'s own, copied from
 * `@codemirror/legacy-modes/mode/sql.js` at the version in `package.json`. They
 * are data, not logic; if the upstream mode ever gains a `backslashStringEscapes`
 * of its own, this whole constant becomes `plSQL` again.
 */
const oracleMode = sql({
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
  // The one line that is ours. Oracle has no backslash escape in a string
  // literal; the mode's default says it does.
  backslashStringEscapes: false,
});

/** The mode's own "space-separated list → lookup" helper, which it does not export. */
function set(words: string): Record<string, boolean> {
  return Object.fromEntries(words.split(' ').map((word) => [word, true]));
}

const HIGHLIGHT: Record<Dialect, LanguageDescriptor['cmExtension']> = {
  oracle: StreamLanguage.define(oracleMode),
  postgres: StreamLanguage.define(pgSQL),
};

/**
 * What a **portable** script is highlighted as.
 *
 * PostgreSQL's mode, deliberately, and it is not an arbitrary pick between two
 * wrongs. A portable file is one that must run on both engines, so it is written
 * in the intersection — and of the two modes, `pgSQL` is the one whose string
 * rules cover that intersection without inventing anything: it already has
 * `backslashStringEscapes: false`, it understands `''` doubling, and the Oracle
 * form it lacks (`q'[…]'`) cannot appear in a file that has to run on PostgreSQL
 * too.
 *
 * Before this, portable and unclassified files fell through to Oracle silently,
 * which is how a whole `COMMON` folder ended up highlighted by the dialect it is
 * specifically not written in.
 */
const PORTABLE_HIGHLIGHT = StreamLanguage.define(pgSQL);

/**
 * Descriptors are cached per `(dialect, connection)` pair.
 *
 * Identity matters: `CodeEditor` builds its extensions from the descriptor at
 * mount, so handing it a freshly-allocated object on every reactive read would
 * churn the editor for no reason. Two keys and not one because the intelligence is
 * bound to a connection — the same Oracle dialect against two databases is two
 * different sets of facts.
 */
const descriptors = new Map<string, LanguageDescriptor>();

/**
 * The descriptor for a dialect, optionally bound to a connection's catalogue.
 *
 * `connectionId` is what turns colouring into intelligence: with it, completion
 * offers this database's tables and columns, hover states their types and the
 * diagnostics can tell an unknown table from an unread schema. Without it the
 * editor still completes keywords and closes blocks, and reports nothing about
 * objects — which is the correct behaviour for a script file with no database
 * open, and much better than measuring it against somebody else's schema.
 *
 * Defaults to Oracle when the dialect is unknown (an unbound query tab still needs
 * to highlight something sensible).
 */
export function sqlLanguage(
  dialect: Dialect | null | undefined,
  connectionId?: string,
): LanguageDescriptor {
  // `null` is a portable script, or one nobody has classified — not Oracle. It
  // used to fall through to Oracle silently, which is how a whole `COMMON` folder
  // came to be highlighted by the one dialect it is specifically not written in.
  const resolved: Dialect | 'portable' =
    dialect === 'postgres' ? 'postgres' : dialect === 'oracle' ? 'oracle' : 'portable';
  const key = `${resolved}|${connectionId ?? ''}`;
  const cached = descriptors.get(key);
  if (cached) return cached;

  const descriptor: LanguageDescriptor = {
    id: `sql-${resolved}`,
    cmExtension: resolved === 'portable' ? PORTABLE_HIGHLIGHT : HIGHLIGHT[resolved],
    // No `commentTokens` here on purpose: a `cmExtension` language already carries
    // its own (the legacy SQL modes declare `--`), so `Ctrl+/` works without one.
    // The intelligence still has to pick a side — the abbreviation expander emits
    // through one dialect's rules and there is no third emitter. PostgreSQL, for
    // the same reason its highlighting is used: a portable script is written in
    // the intersection, and quoting a name the PostgreSQL way is valid in a file
    // that must run there too.
    intel: createSqlIntel(resolved === 'portable' ? 'postgres' : resolved, connectionId),
    ...NO_TREE,
  };
  descriptors.set(key, descriptor);
  return descriptor;
}
