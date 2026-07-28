// Statements that act on the session or on metadata rather than on data:
// COMMENT ON, GRANT/REVOKE, SET, transaction control, CALL.
//
// GRANT/REVOKE are covered structurally only — enough to inventory them and
// splice around them, not enough to reason about privileges.

const { commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');

module.exports = {
  comment_statement: ($) =>
    seq(
      kw('COMMENT'),
      kw('ON'),
      field('object', $._comment_target),
      field('name', $.object_name),
      kw('IS'),
      field(
        'value',
        choice($.string_literal, $.escape_string, $.dollar_quoted_string, $.q_string, $.null_literal),
      ),
    ),

  _comment_target: ($) =>
    choice(
      kw('TABLE'),
      kw('COLUMN'),
      kws('MATERIALIZED', 'VIEW'),
      kw('VIEW'),
      kw('INDEX'),
      kw('SEQUENCE'),
      kw('FUNCTION'),
      kw('PROCEDURE'),
      kw('TRIGGER'),
      kw('SCHEMA'),
      kw('TYPE'),
      kw('CONSTRAINT'),
      kw('DATABASE'),
    ),

  grant_statement: ($) =>
    prec.right(
      seq(
        kw('GRANT'),
        commaSep1($.privilege),
        optional(seq(kw('ON'), optional($._object_type), commaSep1($.object_name))),
        kw('TO'),
        commaSep1($._grantee),
        optional(choice(kws('WITH', 'GRANT', 'OPTION'), kws('WITH', 'ADMIN', 'OPTION'))),
      ),
    ),

  revoke_statement: ($) =>
    prec.right(
      seq(
        kw('REVOKE'),
        optional(kws('GRANT', 'OPTION', 'FOR')),
        commaSep1($.privilege),
        optional(seq(kw('ON'), optional($._object_type), commaSep1($.object_name))),
        kw('FROM'),
        commaSep1($._grantee),
        optional(choice(kw('CASCADE'), kw('RESTRICT'))),
      ),
    ),

  privilege: ($) =>
    choice(
      seq(kw('ALL'), optional(kw('PRIVILEGES'))),
      seq(kw('SELECT'), optional(seq('(', commaSep1($._name), ')'))),
      seq(kw('INSERT'), optional(seq('(', commaSep1($._name), ')'))),
      seq(kw('UPDATE'), optional(seq('(', commaSep1($._name), ')'))),
      seq(kw('REFERENCES'), optional(seq('(', commaSep1($._name), ')'))),
      kw('DELETE'),
      kw('TRUNCATE'),
      kw('TRIGGER'),
      kw('EXECUTE'),
      kw('USAGE'),
      kw('CONNECT'),
      kw('CREATE'),
      // TEMPORARY, and every other privilege spelled with an unreserved word,
      // arrives through `object_name` — listing it twice would be ambiguous.
      $.object_name,
    ),

  _grantee: ($) => choice(kw('PUBLIC'), seq(optional(kw('GROUP')), $.object_name)),

  // `SET x = y`, `SET search_path TO a, b`, and the SQL*Plus family
  // (`SET DEFINE OFF`, `SET SERVEROUTPUT ON SIZE 1000000`).
  set_statement: ($) =>
    prec.right(
      seq(
        kw('SET'),
        optional(choice(kw('SESSION'), kw('LOCAL'))),
        choice(
          seq(
            field('name', $.object_name),
            choice('=', kw('TO')),
            commaSep1(choice($._expression, kw('DEFAULT'))),
          ),
          seq(field('name', $.object_name), repeat1($._set_word)),
          field('name', $.object_name),
        ),
      ),
    ),

  _set_word: ($) => choice($.object_name, $.number_literal, $.string_literal, kw('ON')),

  // Every alternative whose tail is optional gets `prec.right`, because the
  // tail keywords can also start the NEXT statement: without it, `COMMIT`
  // followed by a `COMMENT ON …` is ambiguous with `COMMIT COMMENT 'text'`.
  //
  // `BEGIN` is the exception and gets a NEGATIVE precedence. It has to lose to
  // `plsql_body`, which starts with the same keyword: PostgreSQL's `BEGIN;`
  // matters much less than every Oracle `BEGIN … END` in the repository, and a
  // right-associative BEGIN here silently turns each of those blocks into an
  // error node.
  transaction_statement: ($) =>
    choice(
      prec.right(
        seq(
          choice(kw('COMMIT'), kw('ROLLBACK')),
          optional(choice(kw('WORK'), kw('TRANSACTION'))),
          optional(seq(kw('TO'), optional(kw('SAVEPOINT')), $._name)),
          optional(seq(kw('COMMENT'), $.string_literal)),
        ),
      ),
      seq(kw('SAVEPOINT'), $._name),
      seq(kws('RELEASE', 'SAVEPOINT'), $._name),
      prec.right(seq(kws('START', 'TRANSACTION'), repeat($._set_word))),
      prec(-1, seq(kw('BEGIN'), optional(choice(kw('WORK'), kw('TRANSACTION'))))),
      prec.right(
        seq(kw('LOCK'), optional(kw('TABLE')), commaSep1($.object_name), repeat($._set_word)),
      ),
    ),

  call_statement: ($) => seq(choice(kw('CALL'), kw('EXEC')), $._expression),
};
