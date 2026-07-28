// Expressions — the level the whole product is parsed at.
//
// Picus needs expression depth *inside* procedural bodies (it has to see the
// literal in `INSERT … VALUES ('SOGLIA_SCONTO', …)` that sits three blocks deep
// in an Oracle upgrade script), so there is no "opaque body" shortcut anywhere.
//
// Every dialect-exclusive form gets its own named node, never a generic one:
// `oracle_outer_join`, `prior_expression`, `rownum`, `q_string` on the Oracle
// side; `postgres_cast_expression`, `postgres_operator_expression`,
// `array_constructor`, `dollar_quoted_string` on the PostgreSQL side. That is
// what lets a diagnostic say *which* construct is foreign instead of "syntax
// error at line 12".

const { commaSep, commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');
const { PREC } = require('./precedence');

module.exports = {
  _expression: ($) =>
    choice(
      $._literal,
      $.bind_parameter,
      $.rownum,
      $.object_name,
      $.function_call,
      $.parenthesized_expression,
      $.subquery,
      $.unary_expression,
      $.binary_expression,
      $.postgres_operator_expression,
      $.is_expression,
      $.in_expression,
      $.between_expression,
      $.like_expression,
      $.exists_expression,
      $.case_expression,
      $.cast_expression,
      $.postgres_cast_expression,
      $.array_constructor,
      $.subscript_expression,
      $.prior_expression,
      $.oracle_outer_join,
      $.interval_expression,
      $.extract_expression,
    ),

  // ── Operators ────────────────────────────────────────────────────────────

  binary_expression: ($) =>
    choice(
      ...[
        [PREC.or, kw('OR')],
        [PREC.and, kw('AND')],
        [PREC.compare, choice('=', '!=', '<>', '^=', '~=', '<', '>', '<=', '>=')],
        [PREC.concat, '||'],
        [PREC.add, choice('+', '-')],
        [PREC.mul, choice('*', '/', '%', '**', '^')],
      ].map(([precedence, operator]) =>
        prec.left(
          precedence,
          seq(
            field('left', $._expression),
            field('operator', operator),
            field('right', $._expression),
          ),
        ),
      ),
    ),

  // PostgreSQL-only operators (json/jsonb containment and navigation, array
  // overlap). A separate node because `a -> 'k'` in an Oracle file is a report,
  // not a parse failure.
  postgres_operator_expression: ($) =>
    prec.left(
      PREC.pg_op,
      seq(
          field('left', $._expression),
          field('operator', choice('@>', '<@', '->', '->>', '#>', '#>>', '#-', '&&')),
          field('right', $._expression),
        ),
    ),

  unary_expression: ($) =>
    choice(
      prec.right(PREC.not, seq(field('operator', kw('NOT')), field('operand', $._expression))),
      prec.right(
        PREC.unary,
        seq(field('operator', choice('-', '+', '~')), field('operand', $._expression)),
      ),
    ),

  is_expression: ($) =>
    prec.left(
      PREC.compare,
      seq(
          field('left', $._expression),
          kw('IS'),
          optional(kw('NOT')),
          field(
            'right',
            choice(
              $.null_literal,
              $.boolean_literal,
              kw('UNKNOWN'),
              seq(kw('DISTINCT'), kw('FROM'), $._expression),
              seq(kw('OF'), '(', commaSep1($._data_type), ')'),
            ),
          ),
        ),
    ),

  in_expression: ($) =>
    prec.left(
      PREC.compare,
      seq(
          field('left', $._expression),
          optional(kw('NOT')),
          kw('IN'),
          field('right', choice($.subquery, $.parenthesized_expression)),
        ),
    ),

  between_expression: ($) =>
    prec.left(
      PREC.between,
      seq(
          field('value', $._expression),
          optional(kw('NOT')),
          kw('BETWEEN'),
          optional(choice(kw('SYMMETRIC'), kw('ASYMMETRIC'))),
          field('low', $._expression),
          kw('AND'),
          field('high', $._expression),
        ),
    ),

  like_expression: ($) =>
    prec.left(
      PREC.compare,
      seq(
          field('left', $._expression),
          optional(kw('NOT')),
          field(
            'operator',
            choice(kw('LIKE'), kw('ILIKE'), kws('SIMILAR', 'TO'), '~', '~*', '!~', '!~*'),
          ),
          field('right', $._expression),
          optional(seq(kw('ESCAPE'), $._expression)),
        ),
    ),

  exists_expression: ($) => seq(kw('EXISTS'), $.subquery),

  // ── Grouping ─────────────────────────────────────────────────────────────

  // ONE parenthesised form for all three jobs: grouping `(a + b)`, the row
  // constructor `(a, b) IN (…)`, and the value list `x IN (1, 2, 3)`. Three
  // separate rules all starting with `(` cost several megabytes of parse table
  // for a distinction the consumer can make by counting children.
  parenthesized_expression: ($) => seq('(', commaSep1($._expression), ')'),

  subquery: ($) => seq('(', $.select_statement, ')'),

  // ── Calls ────────────────────────────────────────────────────────────────

  // `prec` so that a dotted name followed by `(` is always a call: without it
  // the parser could also reduce the name to a bare column reference and then
  // choke on the parenthesis.
  function_call: ($) =>
    prec(
      PREC.postfix,
      seq(
        field('name', $.object_name),
        '(',
        optional(
          choice(
            $.star_argument,
            seq(
              optional(choice(kw('DISTINCT'), kw('ALL'), kw('UNIQUE'))),
              commaSep1($._function_argument),
              optional($.order_by_clause),
            ),
          ),
        ),
        ')',
        repeat(choice($.within_group_clause, $.filter_clause, $.over_clause)),
        ),
    ),

  star_argument: ($) => '*',

  _function_argument: ($) => choice($.named_argument, $._expression),

  // `p_code => 'X'` — Oracle and PostgreSQL spell named notation the same way.
  named_argument: ($) => seq(field('name', $._name), '=>', field('value', $._expression)),

  within_group_clause: ($) => seq(kws('WITHIN', 'GROUP'), '(', $.order_by_clause, ')'),

  filter_clause: ($) => seq(kw('FILTER'), '(', $.where_clause, ')'),

  over_clause: ($) => seq(kw('OVER'), choice($.window_definition, $._name)),

  // No leading window name inside the parentheses: `OVER (w ORDER BY …)` is
  // exotic, and admitting it makes `OVER (ROWS …)` ambiguous because ROWS and
  // RANGE are unreserved. `OVER w` still works through `over_clause`.
  window_definition: ($) =>
    seq(
      '(',
      optional($.partition_by_clause),
      optional($.order_by_clause),
      optional($.frame_clause),
      ')',
    ),

  partition_by_clause: ($) => seq(kws('PARTITION', 'BY'), commaSep1($._expression)),

  frame_clause: ($) =>
    seq(
      choice(kw('ROWS'), kw('RANGE'), kw('GROUPS')),
      choice($._frame_bound, seq(kw('BETWEEN'), $._frame_bound, kw('AND'), $._frame_bound)),
      optional(
        seq(
          kw('EXCLUDE'),
          choice(kws('CURRENT', 'ROW'), kw('GROUP'), kw('TIES'), kws('NO', 'OTHERS')),
        ),
      ),
    ),

  _frame_bound: ($) =>
    choice(
      kws('UNBOUNDED', 'PRECEDING'),
      kws('UNBOUNDED', 'FOLLOWING'),
      kws('CURRENT', 'ROW'),
      seq($._expression, choice(kw('PRECEDING'), kw('FOLLOWING'))),
    ),

  // ── Special forms ────────────────────────────────────────────────────────

  case_expression: ($) =>
    seq(
      kw('CASE'),
      optional(field('value', $._expression)),
      repeat1($.when_clause),
      optional(seq(kw('ELSE'), field('default', $._expression))),
      kw('END'),
    ),

  when_clause: ($) =>
    seq(kw('WHEN'), field('condition', $._expression), kw('THEN'), field('result', $._expression)),

  cast_expression: ($) =>
    seq(kw('CAST'), '(', field('value', $._expression), kw('AS'), field('type', $._data_type), ')'),

  // PostgreSQL-only shorthand.
  postgres_cast_expression: ($) =>
    prec.left(
      PREC.cast,
      seq(field('value', $._expression), '::', field('type', $._data_type)),
    ),

  array_constructor: ($) =>
    seq(kw('ARRAY'), choice(seq('[', commaSep($._expression), ']'), $.subquery)),

  subscript_expression: ($) =>
    prec.left(
      PREC.postfix,
      seq(
          field('value', $._expression),
          '[',
          field('index', $._expression),
          optional(seq(':', field('upper', $._expression))),
          ']',
        ),
    ),

  // `prec.right`: the qualifier words (DAY, SECOND, …) are unreserved, so
  // `INTERVAL '1' DAY` would otherwise be ambiguous with an interval aliased
  // `DAY`. The qualifier wins.
  interval_expression: ($) =>
    prec.right(
      seq(
          kw('INTERVAL'),
          field('value', choice($.string_literal, $.escape_string, $.number_literal)),
          optional($.interval_qualifier),
        ),
    ),

  extract_expression: ($) =>
    seq(
      kw('EXTRACT'),
      '(',
      field('field', choice($._interval_field, $._name)),
      kw('FROM'),
      field('source', $._expression),
      ')',
    ),

  // Deliberately NOT modelled: the keyword-argument spellings
  // `TRIM(LEADING 'x' FROM y)`, `SUBSTRING(x FROM 1 FOR 2)`, `COLLATE`, and the
  // quantified comparison `= ANY (…)`. Each is one more recursive expression
  // rule, and together they cost about five megabytes of generated parse table
  // for forms that barely appear in install scripts. `TRIM(x)`, `SUBSTR(x,1,2)`
  // and `SUBSTRING(x, 1, 2)` all still parse — as ordinary function calls.

  // ── Oracle-only leaves ───────────────────────────────────────────────────

  // The pseudo-column. Reserved in Oracle, so it costs nothing to name it, and
  // naming it is what lets `WHERE ROWNUM <= 10` be reported as "Oracle; use
  // LIMIT / FETCH FIRST" rather than parsed as an unknown column.
  rownum: ($) => kw('ROWNUM'),

  // `CONNECT BY PRIOR mgr = empno`
  prior_expression: ($) => prec.right(PREC.unary, seq(kw('PRIOR'), $._expression)),

  // `t.col(+) = u.col` — the old outer-join marker. One token, so a `(+ )` with
  // a space is not accepted; nobody writes it and the alternative is a lexical
  // hazard around every parenthesised unary plus.
  oracle_outer_join: ($) =>
    prec(PREC.postfix, seq($.object_name, alias(token(seq('(', '+', ')')), '(+)'))),
};
