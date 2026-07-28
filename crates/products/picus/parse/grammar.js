/**
 * picus_sql — ONE permissive superset of Oracle (SQL + PL/SQL) and PostgreSQL
 * (SQL + PL/pgSQL).
 *
 * Why one grammar and not two, which is the decision the whole design hangs on:
 * the two dialects diverge almost always **by addition** and almost never by
 * collision. With two strict grammars, an Oracle-ism inside a PostgreSQL file
 * becomes a parser ERROR and the best message available is "syntax error at
 * line 12". With one permissive grammar it becomes **a node with a name**, and
 * the message can be "`MERGE … FROM DUAL` is Oracle syntax; PostgreSQL wants
 * `INSERT … ON CONFLICT`". Diagnosing cross-dialect drift is the product's
 * entire reason to exist, so the grammar's job is to make every divergence
 * *nameable*.
 *
 * The rule that follows from it: **a construct that exists in only one dialect
 * gets its own named node**, never a fold into a generic one. `q_string`,
 * `dual_reference`, `connect_by_clause`, `start_with_clause`, `rownum`,
 * `oracle_outer_join`, `prior_expression`, `percent_type`, `percent_rowtype`,
 * `execute_immediate_statement`, `slash_terminator` and the package rules are
 * Oracle; `dollar_quoted_string`, `escape_string`, `unicode_string`,
 * `postgres_cast_expression`, `postgres_operator_expression`,
 * `array_constructor`, `on_conflict_clause`, `do_statement`, `perform_statement`,
 * `limit_clause`, `offset_clause`, `on_update_action` and `create_schema` are
 * PostgreSQL. `src/dialect.rs` is the table that turns those names into advice.
 *
 * Division of labour with the external scanner (`src/scanner.c`): the scanner
 * owns everything a context-free token cannot express — nesting block comments,
 * Oracle `q'[…]'` with its mirrored delimiters, PostgreSQL `$tag$…$tag$` with
 * its matching tag, and the lone `/` on its own line that terminates an Oracle
 * block without being a division. Everything else is here.
 *
 * Files: `grammar/keywords.js` (case-insensitive keyword tokens and the
 * unreserved list), `lexical.js`, `expression.js`, `query.js`, `dml.js`,
 * `ddl.js`, `routine.js`, `plsql.js`, `session.js`.
 */

const lexical = require('./grammar/lexical');
const expression = require('./grammar/expression');
const query = require('./grammar/query');
const dml = require('./grammar/dml');
const ddl = require('./grammar/ddl');
const routine = require('./grammar/routine');
const plsql = require('./grammar/plsql');
const session = require('./grammar/session');

module.exports = grammar({
  name: 'picus_sql',

  // Order MUST match the `TokenType` enum in `src/scanner.c`.
  externals: ($) => [
    $.block_comment, // `/* … */`, nesting (PostgreSQL); also carries Oracle hints
    $.q_string, // `q'[…]'` and friends
    $.dollar_quoted_string, // `$$…$$` / `$tag$…$tag$`
    $.slash_terminator, // a lone `/` on its own line
    $._error_sentinel, // never emitted; lets the scanner detect error recovery
  ],

  extras: ($) => [/\s+/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  // No GLR conflicts. Every ambiguity in this grammar is settled by precedence
  // instead, and each `prec` carries the reason at its definition — the policy
  // throughout is "a construct prefers to keep going", because the statement
  // terminator is optional and stopping early silently truncates.
  conflicts: () => [],

  rules: Object.assign(
    {
      // ── File structure ─────────────────────────────────────────────────

      source_file: ($) => repeat($._top_level),

      _top_level: ($) => choice($.statement, $.slash_terminator),

      // A statement owns its terminator. That is deliberate: `picus-rewrite`
      // deletes or replaces whole statements, and a range that stops before the
      // `;` (or before the Oracle `/`) leaves an orphan behind.
      // The terminator is OPTIONAL, and that is a decision about the editor:
      // Picus parses live buffers, and `SELECT 1` with the `;` not yet typed
      // must be a statement rather than one big error node. The price is that
      // "where does this statement stop" is no longer decided by a token, which
      // is why so many rules carry `prec.right` — the policy everywhere is that
      // a construct keeps going as long as it can.
      statement: ($) =>
        prec.right(seq($._statement_body, optional(';'), optional($.slash_terminator))),

      _statement_body: ($) =>
        choice(
          $.select_statement,
          $.insert_statement,
          $.update_statement,
          $.delete_statement,
          $.merge_statement,
          $.truncate_statement,
          $.create_table_statement,
          $.create_view_statement,
          $.create_materialized_view_statement,
          $.create_index_statement,
          $.create_sequence_statement,
          $.create_schema_statement,
          $.create_synonym_statement,
          $.create_trigger_statement,
          $.create_function_statement,
          $.create_procedure_statement,
          $.create_package_statement,
          $.create_package_body_statement,
          $.create_type_statement,
          $.alter_table_statement,
          $.alter_sequence_statement,
          $.alter_index_statement,
          $.alter_view_statement,
          $.alter_trigger_statement,
          $.drop_statement,
          $.comment_statement,
          $.grant_statement,
          $.revoke_statement,
          $.set_statement,
          $.transaction_statement,
          $.call_statement,
          $.plsql_block,
          $.do_statement,
        ),
    },
    expression,
    query,
    dml,
    ddl,
    routine,
    plsql,
    session,
    // `lexical` is merged LAST, and that position is load-bearing. SQL keywords
    // are case-insensitive, so they are character-class patterns rather than
    // string literals — which means tree-sitter's keyword-extraction machinery
    // (`word`) cannot capture them, and `identifier` competes with every keyword
    // in the main lexer. Tree-sitter breaks such a tie by, in order: explicit
    // precedence, match LENGTH, then declaration order. Explicit precedence is
    // unusable here (it outranks length, so a `DATA` keyword would win against
    // the longer identifier `DATA_MOD`), so the tie-break has to be declaration
    // order — and every keyword must therefore be declared before `identifier`.
    // Move this line up and `SELECT 1 FROM t` silently reads FROM as an alias.
    lexical,
  ),
});
