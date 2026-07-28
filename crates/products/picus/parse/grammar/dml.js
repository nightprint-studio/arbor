// INSERT / UPDATE / DELETE / MERGE / TRUNCATE.
//
// This is the shape `picus-analyze` reads to find duplicate keys and
// column-less INSERTs, so the pieces it needs are named and fielded: the target
// table, the explicit column list (whose *absence* is itself the finding), each
// VALUES row, each assignment, and the WHERE.

const { commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');

module.exports = {
  insert_statement: ($) =>
    prec.right(
      seq(
        optional($.with_clause),
        kw('INSERT'),
        kw('INTO'),
        field('table', $.object_name),
        optional($._table_alias),
        optional(field('columns', $.column_list)),
        choice(field('values', $.values_clause), field('query', $.select_statement), kws('DEFAULT', 'VALUES')),
        // No bare INTO here: Oracle's `RETURNING … INTO` is part of
        // `returning_clause`, and a second route would be ambiguous with the
        // `SELECT … INTO` of an `INSERT … SELECT`.
        repeat(choice($.on_conflict_clause, $.returning_clause)),
      ),
    ),

  column_list: ($) => seq('(', commaSep1($._name), ')'),

  values_clause: ($) => seq(kw('VALUES'), commaSep1($.value_row)),

  value_row: ($) => seq('(', commaSep1(choice($._expression, kw('DEFAULT'))), ')'),

  // PostgreSQL-only. The Oracle branch of the same change is a MERGE, which is
  // exactly the pair the cross-dialect diff has to recognise.
  on_conflict_clause: ($) =>
    seq(
      kws('ON', 'CONFLICT'),
      optional($._conflict_target),
      kw('DO'),
      choice(
        kw('NOTHING'),
        seq(kw('UPDATE'), kw('SET'), commaSep1($.assignment), optional($.where_clause)),
      ),
    ),

  _conflict_target: ($) =>
    choice(
      seq('(', commaSep1($._expression), ')', optional($.where_clause)),
      seq(kws('ON', 'CONSTRAINT'), $._name),
    ),

  // `RETURNING …` in PostgreSQL, `RETURNING … INTO v` in Oracle PL/SQL.
  returning_clause: ($) =>
    prec.right(seq(kw('RETURNING'), commaSep1($.select_item), optional($.into_clause))),

  update_statement: ($) =>
    prec.right(
      seq(
        optional($.with_clause),
        kw('UPDATE'),
        optional(kw('ONLY')),
        field('table', $.object_name),
        optional($._table_alias),
        kw('SET'),
        commaSep1($.assignment),
        optional($.from_clause),
        optional($.where_clause),
        optional($.returning_clause),
      ),
    ),

  assignment: ($) =>
    seq(
      field('column', choice($.object_name, $.column_list)),
      '=',
      field('value', choice($._expression, kw('DEFAULT'))),
    ),

  delete_statement: ($) =>
    prec.right(
      seq(
        optional($.with_clause),
        kw('DELETE'),
        optional(kw('FROM')),
        optional(kw('ONLY')),
        field('table', $.object_name),
        optional($._table_alias),
        optional($.using_clause),
        optional($.where_clause),
        optional($.returning_clause),
      ),
    ),

  using_clause: ($) => seq(kw('USING'), commaSep1($._table_expression)),

  // Oracle's upsert. Standard since SQL:2003 and now in PostgreSQL 15 too, so
  // the node itself is neutral — what marks the Oracle idiom is the
  // `dual_reference` in its USING sub-select.
  merge_statement: ($) =>
    prec.right(
      seq(
        kw('MERGE'),
        optional(kw('INTO')),
        field('target', $.object_name),
        optional($._table_alias),
        kw('USING'),
        field('source', choice($.subquery, $.object_name)),
        optional($._table_alias),
        kw('ON'),
        field('condition', $._expression),
        repeat1($.merge_when_clause),
        optional($.returning_clause),
      ),
    ),

  merge_when_clause: ($) =>
    choice(
      seq(
          kws('WHEN', 'MATCHED'),
          optional(seq(kw('AND'), $._expression)),
          kw('THEN'),
          choice(
            seq(kw('UPDATE'), kw('SET'), commaSep1($.assignment), optional($.where_clause)),
            kw('DELETE'),
          ),
        ),
      seq(
          kws('WHEN', 'NOT', 'MATCHED'),
          optional(seq(kw('AND'), $._expression)),
          kw('THEN'),
          kw('INSERT'),
          optional($.column_list),
          choice($.values_clause, kws('DEFAULT', 'VALUES')),
          optional($.where_clause),
        ),
    ),

  truncate_statement: ($) =>
    prec.right(
      seq(
        kw('TRUNCATE'),
        optional(kw('TABLE')),
        commaSep1($.object_name),
        optional(
          choice(
            kws('DROP', 'STORAGE'),
            kws('REUSE', 'STORAGE'),
            kws('RESTART', 'IDENTITY'),
            kws('CONTINUE', 'IDENTITY'),
          ),
        ),
        optional(choice(kw('CASCADE'), kw('RESTRICT'))),
      ),
    ),
};
