// Queries: SELECT, its clauses, joins, set operations and CTEs.
//
// ORDER BY / LIMIT / OFFSET / FETCH / FOR UPDATE hang off `select_statement`
// rather than off `select_core`, because in `a UNION b ORDER BY c` the ordering
// belongs to the whole set operation, not to its last operand.

const { commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');

module.exports = {
  select_statement: ($) =>
    seq(
      optional($.with_clause),
      $._query_expression,
      repeat(
        choice(
          $.order_by_clause,
          $.limit_clause,
          $.offset_clause,
          $.fetch_clause,
          $.for_update_clause,
        ),
      ),
    ),

  // `(SELECT …)` is both a parenthesised query operand and a parenthesised
  // expression. The negative precedence settles `((SELECT 1))` in favour of the
  // expression reading, which is the one a select list wants; the operand
  // reading stays reachable where no expression is legal, e.g. around a UNION.
  _query_expression: ($) =>
    choice($.select_core, $.set_operation, prec(-1, $.subquery), prec(-1, $.values_clause)),

  set_operation: ($) =>
    prec.left(
      seq(
          field('left', $._query_expression),
          field(
            'operator',
            choice(
              seq(kw('UNION'), optional(choice(kw('ALL'), kw('DISTINCT')))),
              seq(kw('INTERSECT'), optional(choice(kw('ALL'), kw('DISTINCT')))),
              seq(kw('EXCEPT'), optional(choice(kw('ALL'), kw('DISTINCT')))),
              // Oracle's spelling of EXCEPT.
              kw('MINUS'),
            ),
          ),
          field('right', $._query_expression),
        ),
    ),

  // `prec.right`: every clause after the select list is optional and the
  // statement terminator is optional too, so "keep going" has to beat "stop
  // here and let the next statement start with this keyword". Longest match is
  // the only policy that does not silently truncate a query.
  select_core: ($) =>
    prec.right(
      seq(
        kw('SELECT'),
        optional(
          choice(
            kw('ALL'),
            seq(kw('DISTINCT'), kw('ON'), '(', commaSep1($._expression), ')'),
            kw('DISTINCT'),
            // Oracle's deprecated synonym for DISTINCT.
            kw('UNIQUE'),
          ),
        ),
        field('list', $.select_list),
        optional($.into_clause),
        optional($.from_clause),
        optional($.where_clause),
        repeat(choice($.start_with_clause, $.connect_by_clause)),
        optional($.group_by_clause),
        optional($.having_clause),
        optional($.window_clause),
        ),
    ),

  select_list: ($) => commaSep1($.select_item),

  // `prec.right`: an unreserved word right after an expression is its alias
  // (`SELECT valore VALUE`), not the start of something else.
  select_item: ($) => choice($.all_columns, seq($._expression, optional($._alias))),

  // `t.*` uses a single `.*` token rather than `.` + `*`. Otherwise every dotted
  // name in a select list would need a GLR split just to find out whether the
  // next dot introduces another name component or the star.
  all_columns: ($) => choice('*', seq($.object_name, alias(token(seq('.', '*')), '.*'))),

  _alias: ($) => seq(optional(kw('AS')), field('alias', $._name)),

  // Oracle `SELECT … INTO v_x` / `BULK COLLECT INTO`, PostgreSQL `SELECT … INTO
  // new_table`. Same slot, same node: the shapes agree even though the meanings
  // differ, and the difference is the caller's business.
  into_clause: ($) =>
    seq(optional(kws('BULK', 'COLLECT')), kw('INTO'), commaSep1($._expression)),

  // ── FROM ─────────────────────────────────────────────────────────────────

  from_clause: ($) => seq(kw('FROM'), commaSep1($._table_expression)),

  _table_expression: ($) =>
    choice(
      $.dual_reference,
      $.table_reference,
      $.join_clause,
      $.derived_table,
      $.function_table,
      $.lateral_table,
      $.parenthesized_table,
    ),

  // `prec.right`: an unreserved word after the table name is its alias.
  table_reference: ($) =>
    prec.right(
      seq(optional(kw('ONLY')), field('name', $.object_name), optional($._table_alias)),
    ),

  // Oracle's one-row table. A named node so `FROM DUAL` in a PostgreSQL file is
  // a sentence ("Oracle's one-row table; PostgreSQL omits FROM entirely") and
  // not a missing-relation error at run time.
  dual_reference: ($) => prec.right(seq(kw('DUAL'), optional($._table_alias))),

  derived_table: ($) => prec.right(seq($.subquery, optional($._derived_alias))),

  function_table: ($) => prec.right(seq($.function_call, optional($._derived_alias))),

  lateral_table: ($) =>
    prec.right(
      seq(kw('LATERAL'), choice($.subquery, $.function_call), optional($._derived_alias)),
    ),

  parenthesized_table: ($) =>
    prec.right(seq('(', $._table_expression, ')', optional($._table_alias))),

  _table_alias: ($) => seq(optional(kw('AS')), field('alias', $._name)),

  // Column aliases belong to *derived* tables only. Allowing them after a plain
  // table name would make `INSERT INTO t AS x (a, b) VALUES …` ambiguous, and
  // the wrong reading turns the INSERT's column list into an alias list — the
  // single most load-bearing piece of information in the whole crate.
  _derived_alias: ($) =>
    prec.right(seq(optional(kw('AS')), field('alias', $._name), optional($.column_aliases))),

  column_aliases: ($) => seq('(', commaSep1($._name), ')'),

  join_clause: ($) =>
    prec.left(
      seq(
          field('left', $._table_expression),
          optional(kw('NATURAL')),
          optional($._join_type),
          kw('JOIN'),
          field('right', $._table_expression),
          optional($._join_condition),
        ),
    ),

  _join_type: ($) =>
    choice(
      kw('INNER'),
      seq(choice(kw('LEFT'), kw('RIGHT'), kw('FULL')), optional(kw('OUTER'))),
      kw('CROSS'),
    ),

  _join_condition: ($) =>
    choice(
      seq(kw('ON'), field('condition', $._expression)),
      seq(kw('USING'), '(', commaSep1($._name), ')'),
    ),

  // ── Filtering and grouping ───────────────────────────────────────────────

  where_clause: ($) =>
    seq(kw('WHERE'), choice(field('condition', $._expression), seq(kws('CURRENT', 'OF'), $.object_name))),

  group_by_clause: ($) => seq(kws('GROUP', 'BY'), commaSep1($._grouping_element)),

  _grouping_element: ($) =>
    choice($.rollup_clause, $.cube_clause, $.grouping_sets_clause, $._expression),

  rollup_clause: ($) => seq(kw('ROLLUP'), '(', commaSep1($._expression), ')'),

  cube_clause: ($) => seq(kw('CUBE'), '(', commaSep1($._expression), ')'),

  // The `(a, b)` groups arrive as `parenthesized_expression` through
  // `_expression`.
  grouping_sets_clause: ($) =>
    seq(kws('GROUPING', 'SETS'), '(', commaSep1($._expression), ')'),

  having_clause: ($) => seq(kw('HAVING'), $._expression),

  window_clause: ($) =>
    seq(kw('WINDOW'), commaSep1(seq($._name, kw('AS'), $.window_definition))),

  // ── Oracle hierarchical queries ──────────────────────────────────────────

  start_with_clause: ($) => seq(kws('START', 'WITH'), $._expression),

  connect_by_clause: ($) => seq(kws('CONNECT', 'BY'), optional(kw('NOCYCLE')), $._expression),

  // ── Ordering and paging ──────────────────────────────────────────────────

  order_by_clause: ($) =>
    seq(kw('ORDER'), optional(kw('SIBLINGS')), kw('BY'), commaSep1($.order_by_item)),

  order_by_item: ($) =>
    seq(
      $._expression,
      optional(choice(kw('ASC'), kw('DESC'))),
      optional(seq(kw('NULLS'), choice(kw('FIRST'), kw('LAST')))),
    ),

  // PostgreSQL-only: Oracle spells this FETCH FIRST … ROWS ONLY.
  limit_clause: ($) => seq(kw('LIMIT'), choice($._expression, kw('ALL'))),

  offset_clause: ($) =>
    seq(kw('OFFSET'), $._expression, optional(choice(kw('ROW'), kw('ROWS')))),

  // `prec` because FIRST / NEXT / ROW / ROWS are all unreserved: inside a FETCH
  // they must be read as the clause's own keywords, not as a count expression.
  fetch_clause: ($) =>
    prec(
      1,
      seq(
        kw('FETCH'),
        choice(kw('FIRST'), kw('NEXT')),
        optional($._expression),
        optional(choice(kw('ROW'), kw('ROWS'))),
        choice(kw('ONLY'), kws('WITH', 'TIES')),
        ),
    ),

  for_update_clause: ($) =>
    seq(
      kw('FOR'),
      choice(kw('UPDATE'), kw('SHARE'), kws('NO', 'KEY', 'UPDATE'), kws('KEY', 'SHARE')),
      optional(seq(kw('OF'), commaSep1($.object_name))),
      optional(choice(kw('NOWAIT'), kws('SKIP', 'LOCKED'), seq(kw('WAIT'), $.number_literal))),
    ),

  // ── CTEs ─────────────────────────────────────────────────────────────────

  with_clause: ($) =>
    seq(kw('WITH'), optional(kw('RECURSIVE')), commaSep1($.common_table_expression)),

  common_table_expression: ($) =>
    seq(
      field('name', $._name),
      optional($.column_aliases),
      kw('AS'),
      optional(seq(optional(kw('NOT')), kw('MATERIALIZED'))),
      '(',
      field(
        'query',
        choice($.select_statement, $.insert_statement, $.update_statement, $.delete_statement),
      ),
      ')',
    ),
};
