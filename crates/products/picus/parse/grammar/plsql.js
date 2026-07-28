// Procedural code: Oracle PL/SQL blocks and PostgreSQL PL/pgSQL bodies.
//
// This is the half most SQL grammars skip, and skipping it is exactly what
// Picus cannot afford: a real upgrade script wraps its whole payload in
// `DECLARE … BEGIN … END; /`, so a parser that treats the block as an opaque
// token sees no INSERT, no table, no literal — nothing the product exists to
// analyse. Every statement inside is parsed to full expression depth.
//
// A `_plsql_statement` carries its own `;`. The top-level `statement` wrapper
// carries the one after the block's `END`, which keeps the terminator with the
// statement a rewriter would delete.

const { commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');

module.exports = {
  // `[DECLARE …] BEGIN … [EXCEPTION …] END [name]`
  plsql_block: ($) => seq(optional($.declare_section), $.plsql_body),

  declare_section: ($) => seq(kw('DECLARE'), repeat($._declaration)),

  plsql_body: ($) =>
    prec.right(
      seq(
        kw('BEGIN'),
        repeat($._plsql_statement),
        optional($.exception_section),
        kw('END'),
        optional($._name),
      ),
    ),

  exception_section: ($) => seq(kw('EXCEPTION'), repeat1($.exception_handler)),

  exception_handler: ($) =>
    seq(
      kw('WHEN'),
      field('condition', choice(kw('OTHERS'), seq($.object_name, repeat(seq(kw('OR'), $.object_name))))),
      kw('THEN'),
      repeat($._plsql_statement),
    ),

  // PostgreSQL's anonymous block. `DO $$ … $$` — the body is a dollar-quoted
  // string, so it is one token here; a caller that wants its contents re-parses
  // the inner range with the same parser.
  do_statement: ($) =>
    seq(
      kw('DO'),
      optional($.language_clause),
      field('body', choice($.dollar_quoted_string, $.string_literal)),
      optional($.language_clause),
    ),

  // ── Declarations ─────────────────────────────────────────────────────────

  _declaration: ($) =>
    choice(
      $.cursor_declaration,
      $.exception_declaration,
      $.subtype_declaration,
      $.record_type_declaration,
      $.nested_routine_declaration,
      $.variable_declaration,
    ),

  variable_declaration: ($) =>
    seq(
      field('name', $._name),
      optional(kw('CONSTANT')),
      field('type', $._data_type),
      optional($.not_null_constraint),
      optional(seq(choice(':=', kw('DEFAULT')), field('default', $._expression))),
      ';',
    ),

  cursor_declaration: ($) =>
    seq(
      kw('CURSOR'),
      field('name', $._name),
      optional($.parameter_list),
      optional(seq(kw('RETURN'), $._data_type)),
      optional(seq(kw('IS'), field('query', $.select_statement))),
      ';',
    ),

  exception_declaration: ($) => seq(field('name', $._name), kw('EXCEPTION'), ';'),

  subtype_declaration: ($) =>
    seq(kw('SUBTYPE'), field('name', $._name), kw('IS'), $._data_type, optional($.not_null_constraint), ';'),

  // `prec` because TYPE is an unreserved word: `TYPE x IS …` must win over
  // reading TYPE as the name of a variable declaration.
  record_type_declaration: ($) =>
    prec(
      1,
      seq(
        kw('TYPE'),
        field('name', $._name),
        kw('IS'),
        choice(
          seq(kw('RECORD'), '(', commaSep1(seq($._name, $._data_type, optional(seq(':=', $._expression)))), ')'),
          seq(kws('REF', 'CURSOR'), optional(seq(kw('RETURN'), $._data_type))),
          seq(kws('TABLE', 'OF'), $._data_type, optional(seq(kws('INDEX', 'BY'), $._data_type))),
          seq(kw('VARRAY'), '(', $.number_literal, ')', kw('OF'), $._data_type),
        ),
        ';',
        ),
    ),

  // A procedure or function declared inside a block or a package.
  nested_routine_declaration: ($) =>
    seq(
      choice(kw('PROCEDURE'), kw('FUNCTION')),
      field('name', $._name),
      optional($.parameter_list),
      optional(seq(choice(kw('RETURN'), kw('RETURNS')), $._return_type)),
      repeat($._routine_attribute),
      optional(seq(choice(kw('IS'), kw('AS')), $.plsql_routine_body)),
      ';',
    ),

  // ── Statements ───────────────────────────────────────────────────────────

  _plsql_statement: ($) => seq($._plsql_statement_body, ';'),

  _plsql_statement_body: ($) =>
    choice(
      $.plsql_block,
      $.if_statement,
      $.case_statement,
      $.loop_statement,
      $.while_statement,
      $.for_statement,
      $.forall_statement,
      $.exit_statement,
      $.continue_statement,
      $.return_statement,
      $.raise_statement,
      $.null_statement,
      $.goto_statement,
      $.execute_immediate_statement,
      $.perform_statement,
      $.open_statement,
      $.fetch_statement,
      $.close_statement,
      $.assignment_statement,
      $.select_statement,
      $.insert_statement,
      $.update_statement,
      $.delete_statement,
      $.merge_statement,
      $.transaction_statement,
      $.set_statement,
      $.call_statement,
      $.procedure_call_statement,
    ),

  assignment_statement: ($) =>
    seq(field('target', $._expression), ':=', field('value', $._expression)),

  if_statement: ($) =>
    seq(
      kw('IF'),
      field('condition', $._expression),
      kw('THEN'),
      repeat($._plsql_statement),
      repeat($.elsif_clause),
      optional($.else_clause),
      kws('END', 'IF'),
    ),

  elsif_clause: ($) =>
    seq(
      choice(kw('ELSIF'), kw('ELSEIF')),
      field('condition', $._expression),
      kw('THEN'),
      repeat($._plsql_statement),
    ),

  else_clause: ($) => seq(kw('ELSE'), repeat($._plsql_statement)),

  // `CASE … WHEN … THEN <statements> END CASE` — the statement form, which is
  // not the expression form in `expression.js`.
  case_statement: ($) =>
    seq(
      kw('CASE'),
      optional(field('value', $._expression)),
      repeat1($.case_statement_when),
      optional(seq(kw('ELSE'), repeat($._plsql_statement))),
      kws('END', 'CASE'),
    ),

  case_statement_when: ($) =>
    seq(kw('WHEN'), field('condition', $._expression), kw('THEN'), repeat1($._plsql_statement)),

  loop_statement: ($) =>
    prec.right(
      seq(optional($.statement_label), kw('LOOP'), repeat($._plsql_statement), kws('END', 'LOOP'), optional($._name)),
    ),

  while_statement: ($) =>
    prec.right(
      seq(
        optional($.statement_label),
        kw('WHILE'),
        field('condition', $._expression),
        kw('LOOP'),
        repeat($._plsql_statement),
        kws('END', 'LOOP'),
        optional($._name),
      ),
    ),

  for_statement: ($) =>
    prec.right(
      seq(
        optional($.statement_label),
        kw('FOR'),
        field('variable', $._name),
        kw('IN'),
        optional(kw('REVERSE')),
        // A cursor loop's `(SELECT …)` arrives through `_expression` → `subquery`;
        // listing `subquery` here as well would be ambiguous with it.
        field('range', choice($.numeric_range, $._expression)),
        kw('LOOP'),
        repeat($._plsql_statement),
        kws('END', 'LOOP'),
        optional($._name),
        ),
    ),

  numeric_range: ($) => seq($._expression, '..', $._expression),

  // Oracle bulk DML.
  forall_statement: ($) =>
    seq(
      kw('FORALL'),
      field('variable', $._name),
      kw('IN'),
      field('range', choice($.numeric_range, $._expression)),
      optional(kws('SAVE', 'EXCEPTIONS')),
      $._plsql_statement_body,
    ),

  statement_label: ($) => seq('<<', $._name, '>>'),

  exit_statement: ($) =>
    seq(kw('EXIT'), optional($._name), optional(seq(kw('WHEN'), $._expression))),

  continue_statement: ($) =>
    seq(kw('CONTINUE'), optional($._name), optional(seq(kw('WHEN'), $._expression))),

  return_statement: ($) => seq(kw('RETURN'), optional(choice($.select_statement, $._expression))),

  // Oracle `RAISE my_error;` and PL/pgSQL `RAISE NOTICE 'x = %', v;` — same
  // keyword, different grammar, one node with an optional level.
  raise_statement: ($) =>
    seq(
      kw('RAISE'),
      optional(
        field(
          'level',
          // `LOG` is missing on purpose: it is a maths function in Oracle
          // (`LOG(10, 100)`), and a keyword would cost more than `RAISE LOG`
          // is worth.
          choice(kw('DEBUG'), kw('INFO'), kw('NOTICE'), kw('WARNING'), kw('EXCEPTION')),
        ),
      ),
      optional(field('name', $.object_name)),
      optional(seq(field('message', choice($.string_literal, $.escape_string, $.dollar_quoted_string)), repeat(seq(',', $._expression)))),
      optional(seq(kw('USING'), commaSep1(seq($._name, '=', $._expression)))),
    ),

  null_statement: ($) => kw('NULL'),

  goto_statement: ($) => seq(kw('GOTO'), $._name),

  // Oracle-only dynamic SQL.
  execute_immediate_statement: ($) =>
    seq(
      kws('EXECUTE', 'IMMEDIATE'),
      field('sql', $._expression),
      optional($.into_clause),
      optional(seq(kw('USING'), commaSep1(seq(optional($._parameter_mode), $._expression)))),
      optional($.returning_clause),
    ),

  // PL/pgSQL-only: run a query and throw the rows away.
  perform_statement: ($) =>
    seq(
      kw('PERFORM'),
      $.select_list,
      optional($.from_clause),
      optional($.where_clause),
      optional($.group_by_clause),
      optional($.having_clause),
      repeat(choice($.order_by_clause, $.limit_clause, $.offset_clause)),
    ),

  open_statement: ($) =>
    seq(
      kw('OPEN'),
      field('cursor', $.object_name),
      optional(seq('(', commaSep1($._expression), ')')),
      optional(seq(kw('FOR'), choice($.select_statement, $._expression))),
    ),

  fetch_statement: ($) =>
    seq(
      kw('FETCH'),
      optional(choice(kw('NEXT'), kw('PRIOR'), kw('FIRST'), kw('LAST'))),
      optional(kw('FROM')),
      field('cursor', $.object_name),
      optional(seq(optional(kws('BULK', 'COLLECT')), kw('INTO'), commaSep1($._expression))),
      optional(seq(kw('LIMIT'), $._expression)),
    ),

  close_statement: ($) => seq(kw('CLOSE'), field('cursor', $.object_name)),

  // `my_pkg.do_it(1, 2);` or `commit_work;`
  procedure_call_statement: ($) => choice($.function_call, $.object_name),
};
