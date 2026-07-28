// Structural DDL: tables, views, materialized views, indexes, sequences,
// schemas, synonyms — plus the constraint vocabulary they share, and DROP.
//
// Routines (function/procedure/package/trigger/type) live in `routine.js`;
// GRANT/COMMENT/SET/transaction control in `session.js`.

const { commaSep1 } = require('./util');
const { ci, kw, kws } = require('./keywords');

module.exports = {
  // ── CREATE TABLE ─────────────────────────────────────────────────────────

  create_table_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(choice(kw('GLOBAL'), kw('LOCAL'))),
        optional(choice(kw('TEMP'), kw('TEMPORARY'), kw('UNLOGGED'))),
        kw('TABLE'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        choice(
          seq('(', commaSep1($._table_item), ')'),
          seq(optional($.column_aliases), kw('AS'), field('query', $.select_statement)),
          seq(kw('OF'), $._data_type),
        ),
        repeat($.table_option),
      ),
    ),

  _table_item: ($) => choice($.column_definition, $.table_constraint),

  column_definition: ($) =>
    seq(field('name', $._name), field('type', $._data_type), repeat($.column_constraint)),

  // `prec.right` on both constraint rules: their trailing attributes
  // (`NOT DEFERRABLE`, `ENABLE`, …) start with words that can also start the
  // next constraint, so each one has to prefer keeping its own tail.
  column_constraint: ($) =>
    prec.right(
      seq(
        optional(seq(kw('CONSTRAINT'), field('name', $._name))),
        choice(
          $.not_null_constraint,
          $.null_constraint,
          $.default_clause,
          $.primary_key_constraint,
          $.unique_constraint,
          $.check_constraint,
          $.references_clause,
          $.generated_clause,
          seq(kw('COLLATE'), $._name),
        ),
        repeat($._constraint_attribute),
        ),
    ),

  table_constraint: ($) =>
    prec.right(
      seq(
        optional(seq(kw('CONSTRAINT'), field('name', $._name))),
        choice(
          $.primary_key_constraint,
          $.unique_constraint,
          $.check_constraint,
          $.foreign_key_constraint,
        ),
        repeat($._constraint_attribute),
        ),
    ),

  // ONE token, not two keywords. `DEFAULT 0 NOT NULL` is the commonest column
  // definition in existence, and with two tokens the parser reaches the NOT
  // still inside the default's expression, where `NOT IN` / `NOT LIKE` /
  // `NOT BETWEEN` are all live and outrank stopping. As a single token,
  // context-aware lexing settles it: `NOT NULL` is longer than `NOT`, so it wins
  // where a constraint may start, and it is not even a valid token inside
  // `a IS NOT NULL`, so that keeps lexing as two words.
  not_null_constraint: ($) => alias(token(seq(ci('NOT'), /\s+/, ci('NULL'))), 'NOT NULL'),

  null_constraint: ($) => kw('NULL'),

  default_clause: ($) => seq(kw('DEFAULT'), field('value', $._expression)),

  // `prec.right`: the column list is optional (column-level constraints have
  // none), so the parser has to prefer taking it when it is there.
  primary_key_constraint: ($) =>
    prec.right(seq(kws('PRIMARY', 'KEY'), optional(seq('(', commaSep1($._name), ')')))),

  unique_constraint: ($) =>
    prec.right(seq(kw('UNIQUE'), optional(seq('(', commaSep1($._name), ')')))),

  check_constraint: ($) => seq(kw('CHECK'), '(', field('condition', $._expression), ')'),

  foreign_key_constraint: ($) =>
    seq(kws('FOREIGN', 'KEY'), '(', commaSep1($._name), ')', $.references_clause),

  references_clause: ($) =>
    prec.right(
      seq(
        kw('REFERENCES'),
        field('table', $.object_name),
        optional(seq('(', commaSep1($._name), ')')),
        repeat(
          choice(
            $.on_delete_action,
            $.on_update_action,
            seq(kw('MATCH'), choice(kw('FULL'), kw('PARTIAL'), kw('SIMPLE'))),
          ),
        ),
        ),
    ),

  on_delete_action: ($) => seq(kws('ON', 'DELETE'), $._referential_action),

  // Oracle has no ON UPDATE, so this node is one of the cheap cross-dialect
  // signals: seeing it in an Oracle file is a finding, not a parse error.
  on_update_action: ($) => seq(kws('ON', 'UPDATE'), $._referential_action),

  _referential_action: ($) =>
    choice(
      kw('CASCADE'),
      kws('SET', 'NULL'),
      kws('SET', 'DEFAULT'),
      kw('RESTRICT'),
      kws('NO', 'ACTION'),
    ),

  generated_clause: ($) =>
    prec.right(
      seq(
        kw('GENERATED'),
        optional(choice(kw('ALWAYS'), kws('BY', 'DEFAULT'))),
        kw('AS'),
        choice(
          seq('(', $._expression, ')', optional(kw('STORED'))),
          seq(kw('IDENTITY'), optional(seq('(', repeat($.sequence_option), ')'))),
        ),
        ),
    ),

  _constraint_attribute: ($) =>
    choice(
      kw('DEFERRABLE'),
      // `NOT DEFERRABLE` is deliberately absent. A constraint keeps its trailing
      // attributes greedily (see `prec.right` above), so admitting an attribute
      // that starts with NOT would make `DEFAULT 0 NOT NULL` — the single most
      // common column definition there is — try to read `NOT DEFERRABLE`.
      kws('INITIALLY', 'DEFERRED'),
      kws('INITIALLY', 'IMMEDIATE'),
      kw('ENABLE'),
      kw('DISABLE'),
      kw('VALIDATE'),
      kw('NOVALIDATE'),
      kw('RELY'),
      kw('NORELY'),
      seq(kws('USING', 'INDEX'), optional($.object_name)),
      seq(kw('TABLESPACE'), $._name),
    ),

  table_option: ($) =>
    choice(
      seq(kw('TABLESPACE'), $._name),
      seq(kws('ON', 'COMMIT'), choice(kws('DELETE', 'ROWS'), kws('PRESERVE', 'ROWS'), kws('DROP'))),
      seq(kw('WITH'), '(', commaSep1($._storage_parameter), ')'),
      kws('WITHOUT', 'OIDS'),
      seq(kw('INHERITS'), '(', commaSep1($.object_name), ')'),
      seq(kws('PARTITION', 'BY'), $._name, '(', commaSep1($._expression), ')'),
      kws('ORGANIZATION', 'INDEX'),
      kw('COMPRESS'),
      kw('NOCOMPRESS'),
      kw('LOGGING'),
      kw('NOLOGGING'),
      kw('CACHE'),
      kw('NOCACHE'),
      seq(kw('STORAGE'), '(', repeat1(seq($._name, optional($._expression))), ')'),
      seq(
          choice(kw('PCTFREE'), kw('PCTUSED'), kw('INITRANS'), kw('MAXTRANS')),
          $.number_literal,
        ),
    ),

  _storage_parameter: ($) => seq($.object_name, optional(seq('=', $._expression))),

  // ── CREATE VIEW ──────────────────────────────────────────────────────────

  create_view_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        optional(choice(kw('FORCE'), kws('NO', 'FORCE'))),
        optional(choice(kw('TEMP'), kw('TEMPORARY'))),
        kw('VIEW'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        optional($.column_aliases),
        optional(seq(kw('WITH'), '(', commaSep1($._storage_parameter), ')')),
        kw('AS'),
        field('query', $.select_statement),
        optional(
          choice(
            seq(kw('WITH'), optional(choice(kw('CASCADED'), kw('LOCAL'))), kws('CHECK', 'OPTION')),
            kws('WITH', 'READ', 'ONLY'),
          ),
        ),
      ),
    ),

  create_materialized_view_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        kws('MATERIALIZED', 'VIEW'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        optional($.column_aliases),
        repeat(choice($.table_option, $._mview_option)),
        kw('AS'),
        field('query', $.select_statement),
        optional(choice(kws('WITH', 'DATA'), kws('WITH', 'NO', 'DATA'))),
      ),
    ),

  _mview_option: ($) =>
    choice(
      kws('BUILD', 'IMMEDIATE'),
      kws('BUILD', 'DEFERRED'),
      prec.right(
        seq(
          kw('REFRESH'),
          repeat1(choice(kw('FAST'), kw('COMPLETE'), kw('FORCE'), kws('ON', 'DEMAND'), kws('ON', 'COMMIT'))),
        ),
      ),
      kws('ENABLE', 'QUERY', 'REWRITE'),
      kws('DISABLE', 'QUERY', 'REWRITE'),
    ),

  // ── CREATE INDEX ─────────────────────────────────────────────────────────

  create_index_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(choice(kw('UNIQUE'), kw('BITMAP'))),
        kw('INDEX'),
        optional(kw('CONCURRENTLY')),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        kw('ON'),
        field('table', $.object_name),
        optional(seq(kw('USING'), $._name)),
        '(',
        commaSep1($.index_element),
        ')',
        repeat(
          choice(
            seq(kw('INCLUDE'), '(', commaSep1($._name), ')'),
            $.where_clause,
            $.table_option,
          ),
        ),
      ),
    ),

  index_element: ($) =>
    seq(
      $._expression,
      optional(seq(kw('COLLATE'), $._name)),
      optional(choice(kw('ASC'), kw('DESC'))),
      optional(seq(kw('NULLS'), choice(kw('FIRST'), kw('LAST')))),
    ),

  // ── CREATE SEQUENCE ──────────────────────────────────────────────────────

  create_sequence_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(choice(kw('TEMP'), kw('TEMPORARY'))),
        kw('SEQUENCE'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        field('name', $.object_name),
        repeat($.sequence_option),
      ),
    ),

  // `prec.right`: `RESTART` alone and `RESTART WITH 1` are both legal, so each
  // option has to prefer taking its optional tail.
  sequence_option: ($) =>
    prec.right(choice(
      seq(kw('START'), optional(kw('WITH')), $._expression),
      seq(kw('RESTART'), optional(kw('WITH')), optional($._expression)),
      seq(kw('INCREMENT'), optional(kw('BY')), $._expression),
      seq(kw('MINVALUE'), $._expression),
      seq(kw('MAXVALUE'), $._expression),
      seq(kw('CACHE'), $._expression),
      kw('NOMINVALUE'),
      kw('NOMAXVALUE'),
      kw('NOCACHE'),
      kw('NOCYCLE'),
      kw('NOORDER'),
      kw('CYCLE'),
      kw('ORDER'),
      kws('NO', 'MINVALUE'),
      kws('NO', 'MAXVALUE'),
      kws('NO', 'CACHE'),
      kws('NO', 'CYCLE'),
      seq(kw('AS'), $._data_type),
      seq(kws('OWNED', 'BY'), choice($.object_name, kw('NONE'))),
    )),

  // ── CREATE SCHEMA / SYNONYM ──────────────────────────────────────────────

  create_schema_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        kw('SCHEMA'),
        optional(kws('IF', 'NOT', 'EXISTS')),
        optional(field('name', $.object_name)),
        optional(seq(kw('AUTHORIZATION'), $._name)),
      ),
    ),

  // Oracle-only.
  create_synonym_statement: ($) =>
    prec.right(
      seq(
        kw('CREATE'),
        optional(kws('OR', 'REPLACE')),
        optional(kw('PUBLIC')),
        kw('SYNONYM'),
        field('name', $.object_name),
        kw('FOR'),
        field('target', $.object_name),
      ),
    ),

  // ── ALTER ────────────────────────────────────────────────────────────────

  alter_table_statement: ($) =>
    prec.right(
      seq(
        kw('ALTER'),
        kw('TABLE'),
        optional(kws('IF', 'EXISTS')),
        optional(kw('ONLY')),
        field('name', $.object_name),
        commaSep1($.alter_table_action),
      ),
    ),

  alter_table_action: ($) =>
    choice(
      seq(
          kw('ADD'),
          choice(
            seq(optional(kw('COLUMN')), optional(kws('IF', 'NOT', 'EXISTS')), $.column_definition),
            $.table_constraint,
            seq('(', commaSep1($._table_item), ')'),
          ),
        ),
      seq(
          kw('DROP'),
          choice(
            seq(optional(kw('COLUMN')), optional(kws('IF', 'EXISTS')), $._name),
            seq(kw('CONSTRAINT'), optional(kws('IF', 'EXISTS')), $._name),
            kws('PRIMARY', 'KEY'),
            seq(kw('UNIQUE'), '(', commaSep1($._name), ')'),
          ),
          optional(choice(kw('CASCADE'), kws('CASCADE', 'CONSTRAINTS'), kw('RESTRICT'))),
        ),
      seq(kw('ALTER'), optional(kw('COLUMN')), $._name, $._alter_column_action),
      // Oracle's spelling of ALTER COLUMN.
      seq(
        kw('MODIFY'),
        choice($.column_definition, seq('(', commaSep1($.column_definition), ')')),
      ),
      seq(kw('RENAME'), kw('CONSTRAINT'), $._name, kw('TO'), $._name),
      seq(kw('RENAME'), optional(kw('COLUMN')), $._name, kw('TO'), $._name),
      seq(kw('RENAME'), kw('TO'), $._name),
      seq(
          choice(kw('ENABLE'), kw('DISABLE')),
          optional(choice(kw('VALIDATE'), kw('NOVALIDATE'))),
          choice(
            seq(kw('CONSTRAINT'), $._name),
            seq(kw('TRIGGER'), choice($._name, kw('ALL'))),
            kws('ALL', 'TRIGGERS'),
            kws('ROW', 'MOVEMENT'),
          ),
        ),
      seq(kws('OWNER', 'TO'), $._name),
      seq(kw('SET'), choice(seq(kw('TABLESPACE'), $._name), seq(kw('SCHEMA'), $._name), kw('LOGGED'), kw('UNLOGGED'))),
      $.table_option,
    ),

  _alter_column_action: ($) =>
    choice(
      seq(kws('SET', 'DEFAULT'), $._expression),
      kws('DROP', 'DEFAULT'),
      kws('SET', 'NOT', 'NULL'),
      kws('DROP', 'NOT', 'NULL'),
      seq(
          optional(kws('SET', 'DATA')),
          kw('TYPE'),
          $._data_type,
          optional(seq(kw('USING'), $._expression)),
        ),
      seq(kws('SET', 'STATISTICS'), $.number_literal),
    ),

  alter_sequence_statement: ($) =>
    prec.right(
      seq(
        kw('ALTER'),
        kw('SEQUENCE'),
        optional(kws('IF', 'EXISTS')),
        field('name', $.object_name),
        repeat1(choice($.sequence_option, seq(kws('OWNER', 'TO'), $._name), seq(kw('RENAME'), kw('TO'), $._name))),
      ),
    ),

  alter_index_statement: ($) =>
    prec.right(
      seq(
        kw('ALTER'),
        kw('INDEX'),
        optional(kws('IF', 'EXISTS')),
        field('name', $.object_name),
        repeat1(
          choice(
            seq(kw('RENAME'), kw('TO'), $._name),
            kw('REBUILD'),
            kw('UNUSABLE'),
            seq(kw('SET'), '(', commaSep1($._storage_parameter), ')'),
            $.table_option,
          ),
        ),
      ),
    ),

  alter_view_statement: ($) =>
    prec.right(
      seq(
        kw('ALTER'),
        optional(kw('MATERIALIZED')),
        kw('VIEW'),
        optional(kws('IF', 'EXISTS')),
        field('name', $.object_name),
        repeat1(
          choice(
            seq(kw('RENAME'), kw('TO'), $._name),
            seq(kws('OWNER', 'TO'), $._name),
            kw('COMPILE'),
            seq(kw('SET'), '(', commaSep1($._storage_parameter), ')'),
            $._mview_option,
          ),
        ),
      ),
    ),

  // ── DROP ─────────────────────────────────────────────────────────────────

  drop_statement: ($) =>
    prec.right(
      seq(
        kw('DROP'),
        field('object', $._object_type),
        optional(kw('CONCURRENTLY')),
        optional(kws('IF', 'EXISTS')),
        commaSep1($.object_name),
        optional($.parameter_list),
        repeat(
          choice(
            kws('CASCADE', 'CONSTRAINTS'),
            kw('CASCADE'),
            kw('RESTRICT'),
            kw('PURGE'),
            kw('FORCE'),
          ),
        ),
      ),
    ),

  _object_type: ($) =>
    choice(
      kw('TABLE'),
      kws('MATERIALIZED', 'VIEW'),
      kw('VIEW'),
      kw('INDEX'),
      kw('SEQUENCE'),
      kw('TRIGGER'),
      kw('FUNCTION'),
      kw('PROCEDURE'),
      kws('PACKAGE', 'BODY'),
      kw('PACKAGE'),
      kw('TYPE'),
      kw('SCHEMA'),
      kw('DATABASE'),
      kw('SYNONYM'),
      kw('CONSTRAINT'),
      kw('ROLE'),
      kw('USER'),
      kw('TABLESPACE'),
      kw('DOMAIN'),
      kw('EXTENSION'),
    ),
};
