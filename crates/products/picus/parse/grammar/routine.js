// Stored code: functions, procedures, packages, triggers and user types.
//
// The two dialects wrap the same idea in different envelopes — Oracle's
// `IS … BEGIN … END; /` and PostgreSQL's `AS $$ … $$ LANGUAGE plpgsql;` — but
// the body underneath is procedural code in both, so both routes land on the
// procedural rules in `plsql.js` (or on a `dollar_quoted_string` whose contents
// are re-parsed by the caller when it wants them).

const { commaSep, commaSep1 } = require('./util');
const { kw, kws } = require('./keywords');

module.exports = {
  create_function_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        kw('FUNCTION'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        optional($.parameter_list),
        optional(seq(choice(kw('RETURNS'), kw('RETURN')), field('return_type', $._return_type))),
        repeat(choice($._routine_attribute, $.routine_body)),
      ),
    ),

  create_procedure_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        kw('PROCEDURE'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        optional($.parameter_list),
        repeat(choice($._routine_attribute, $.routine_body)),
      ),
    ),

  _return_type: ($) => choice($.returns_table, seq(kw('SETOF'), $._data_type), $._data_type),

  returns_table: ($) =>
    seq(kw('TABLE'), '(', commaSep1(seq(field('name', $._name), $._data_type)), ')'),

  parameter_list: ($) => seq('(', commaSep($.parameter), ')'),

  // Oracle writes the mode after the name (`p_id IN NUMBER`), PostgreSQL before
  // it (`IN p_id integer`). Accepting both slots is cheaper than two rules and
  // costs nothing: no real script writes a mode twice.
  parameter: ($) =>
    seq(
      optional($._parameter_mode),
      field('name', $._name),
      optional($._parameter_mode),
      field('type', $._data_type),
      optional(seq(choice(kw('DEFAULT'), ':='), field('default', $._expression))),
    ),

  _parameter_mode: ($) =>
    choice(kws('IN', 'OUT'), kw('INOUT'), kw('IN'), kw('OUT'), kw('VARIADIC'), kw('NOCOPY')),

  _routine_attribute: ($) =>
    choice(
      $.language_clause,
      kw('IMMUTABLE'),
      kw('STABLE'),
      kw('VOLATILE'),
      kw('LEAKPROOF'),
      kws('NOT', 'LEAKPROOF'),
      kw('STRICT'),
      kws('CALLED', 'ON', 'NULL', 'INPUT'),
      kws('RETURNS', 'NULL', 'ON', 'NULL', 'INPUT'),
      kws('SECURITY', 'DEFINER'),
      kws('SECURITY', 'INVOKER'),
      kws('EXTERNAL', 'SECURITY', 'DEFINER'),
      kws('EXTERNAL', 'SECURITY', 'INVOKER'),
      seq(kw('PARALLEL'), choice(kw('SAFE'), kw('UNSAFE'), kw('RESTRICTED'))),
      seq(kw('COST'), $.number_literal),
      seq(kw('ROWS'), $.number_literal),
      seq(kw('SET'), $.object_name, choice('=', kw('TO')), commaSep1($._expression)),
      kw('WINDOW'),
      // Oracle
      kw('DETERMINISTIC'),
      kw('PIPELINED'),
      kw('PARALLEL_ENABLE'),
      kw('RESULT_CACHE'),
      kws('AUTHID', 'DEFINER'),
      kws('AUTHID', 'CURRENT_USER'),
    ),

  language_clause: ($) => seq(kw('LANGUAGE'), field('language', $._name)),

  // `AS $$ … $$` (PostgreSQL) or `IS <declarations> BEGIN … END` (Oracle).
  routine_body: ($) =>
    seq(
      choice(kw('AS'), kw('IS')),
      choice(
        field('body', $.dollar_quoted_string),
        field('body', $.string_literal),
        field('body', $.plsql_routine_body),
      ),
    ),

  plsql_routine_body: ($) => seq(repeat($._declaration), $.plsql_body),

  // ── Packages (Oracle only) ───────────────────────────────────────────────

  create_package_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        kw('PACKAGE'),
        field('name', $.object_name),
        optional(choice(kws('AUTHID', 'DEFINER'), kws('AUTHID', 'CURRENT_USER'))),
        choice(kw('IS'), kw('AS')),
        repeat($._declaration),
        kw('END'),
        optional($._name),
        ),
    ),

  create_package_body_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        kws('PACKAGE', 'BODY'),
        field('name', $.object_name),
        choice(kw('IS'), kw('AS')),
        repeat($._declaration),
        optional(
          seq(kw('BEGIN'), repeat($._plsql_statement), optional($.exception_section)),
        ),
        kw('END'),
        optional($._name),
        ),
    ),

  // ── Triggers ─────────────────────────────────────────────────────────────

  create_trigger_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        optional(kw('CONSTRAINT')),
        kw('TRIGGER'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        field('timing', choice(kw('BEFORE'), kw('AFTER'), kws('INSTEAD', 'OF'))),
        field('events', $.trigger_events),
        kw('ON'),
        field('table', $.object_name),
        repeat($._trigger_option),
        field('body', choice($.trigger_execute, $.plsql_block)),
      ),
    ),

  trigger_events: ($) => seq($._trigger_event, repeat(seq(kw('OR'), $._trigger_event))),

  _trigger_event: ($) =>
    choice(
      kw('INSERT'),
      kw('DELETE'),
      kw('TRUNCATE'),
      seq(kw('UPDATE'), optional(seq(kw('OF'), commaSep1($._name)))),
    ),

  _trigger_option: ($) =>
    choice(
      seq(
          kw('REFERENCING'),
          repeat1(
            seq(
              choice(kw('OLD'), kw('NEW')),
              optional(choice(kw('ROW'), kw('TABLE'))),
              optional(kw('AS')),
              $._name,
            ),
          ),
        ),
      kws('FOR', 'EACH', 'ROW'),
      kws('FOR', 'EACH', 'STATEMENT'),
      kw('DEFERRABLE'),
      kws('NOT', 'DEFERRABLE'),
      kws('INITIALLY', 'DEFERRED'),
      kws('INITIALLY', 'IMMEDIATE'),
      seq(kw('WHEN'), '(', $._expression, ')'),
      kw('ENABLE'),
      kw('DISABLE'),
      seq(kw('FOLLOWS'), $.object_name),
      seq(kw('FROM'), $.object_name),
    ),

  // PostgreSQL's trigger body is always a call; Oracle's is inline PL/SQL.
  trigger_execute: ($) =>
    seq(kw('EXECUTE'), choice(kw('PROCEDURE'), kw('FUNCTION')), $.function_call),

  alter_trigger_statement: ($) =>
    prec.right(
      seq(
        kw('ALTER'),
        kw('TRIGGER'),
        field('name', $.object_name),
        repeat1(choice(kw('ENABLE'), kw('DISABLE'), kw('COMPILE'), seq(kw('RENAME'), kw('TO'), $._name))),
      ),
    ),

  // ── Types ────────────────────────────────────────────────────────────────

  // The higher precedence resolves `CREATE TYPE x AS RANGE (…)`: RANGE is
  // unreserved, so it could also be read as the name of a base type.
  create_type_statement: ($) =>
    prec.right(
      1,
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        kw('TYPE'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        optional(
          seq(
            kw('AS'),
            choice(
              seq(kw('ENUM'), '(', commaSep($.string_literal), ')'),
              seq(kw('RANGE'), '(', commaSep1($._storage_parameter), ')'),
              seq(kws('TABLE', 'OF'), $._data_type),
              seq(kws('VARRAY'), '(', $.number_literal, ')', kws('OF'), $._data_type),
              seq(kw('OBJECT'), '(', commaSep1(seq(field('name', $._name), $._data_type)), ')'),
              seq('(', commaSep1(seq(field('name', $._name), $._data_type)), ')'),
              $._data_type,
            ),
          ),
        ),
      ),
    ),
};
