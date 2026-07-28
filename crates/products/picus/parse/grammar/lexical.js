// Names, literals and type references — the leaves both dialects share, plus the
// few leaves only one of them has.
//
// The lexically hostile literals (`q'[…]'`, `$tag$…$tag$`, nesting block
// comments, the lone Oracle `/`) are NOT here: they live in `src/scanner.c`
// because no context-free token can express them. See that file's header.

const { commaSep1 } = require('./util');
const { ci, kw, kws, UNRESERVED } = require('./keywords');

module.exports = {
  // ── Names ────────────────────────────────────────────────────────────────

  _name: ($) =>
    choice($.identifier, $.quoted_identifier, alias($._unreserved_keyword, $.identifier)),

  _unreserved_keyword: ($) => choice(...UNRESERVED.map(kw)),

  // `schema.table`, `pkg.proc`, `t.col`, `t@dblink`. One node for every dotted
  // name: whether it denotes a table, a column or a function is decided by the
  // parent node, not by a second rule with the same shape (which would cost a
  // permanent reduce/reduce conflict for no information).
  object_name: ($) =>
    seq($._name, repeat(seq('.', $._name)), optional(seq('@', $._name))),

  // ── Literals ─────────────────────────────────────────────────────────────

  // `'…'` with `''` doubling. Newlines are allowed inside: a multi-line string
  // is legal, and a `--` inside one is NOT a comment — that falls out of the
  // token being matched before `extras` get a chance.
  string_literal: ($) => /'([^']|'')*'/,

  // PostgreSQL `E'…\n…'`. Backslash escapes make it a different token, not a
  // decoration: `E'a\'b'` ends at the LAST quote, `'a\'b'` at the middle one.
  escape_string: ($) => /[eE]'([^'\\]|\\.|'')*'/,

  // PostgreSQL `U&'\0041'`.
  unicode_string: ($) => /[uU]&'([^']|'')*'/,

  national_string: ($) => /[nN]'([^']|'')*'/,

  bit_string: ($) => /[bBxX]'[^']*'/,

  // `12`, `1.5`, `1.5e3`, `.5`. Deliberately NOT accepting a trailing bare dot:
  // `1.` would make the PL/SQL range `1..10` lex as `1.` `.10`, and ranges are
  // worth more than a spelling nobody writes.
  number_literal: ($) => /(\d+\.\d+|\.\d+|\d+)([eE][+-]?\d+)?/,

  boolean_literal: ($) => choice(kw('TRUE'), kw('FALSE')),

  null_literal: ($) => kw('NULL'),

  // `DATE '2024-01-01'`, `TIMESTAMP '…'`.
  // `prec` because DATE / TIME / TIMESTAMP are unreserved (they have to be — a
  // column of type DATE is spelled `x DATE`): when a string literal follows
  // immediately, the typed-literal reading wins over "a column called DATE".
  typed_literal: ($) =>
    prec(
      1,
      seq(
        field('type', choice(kw('DATE'), kw('TIME'), kw('TIMESTAMP'))),
        field('value', choice($.string_literal, $.escape_string)),
        ),
    ),

  // `:name`, `:1` (Oracle / JDBC), `$1` (PostgreSQL), `?`.
  // The dotted suffix is Oracle's trigger correlation name (`:NEW.col`) and,
  // more generally, a host-variable field reference. Kept inside the token so
  // the range stays exact; the components are not broken out because nothing
  // downstream asks for them.
  bind_parameter: ($) =>
    token(
      choice(/:[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)*/, /:\d+/, /\$\d+/, '?'),
    ),

  _literal: ($) =>
    choice(
      $.string_literal,
      $.escape_string,
      $.unicode_string,
      $.national_string,
      $.bit_string,
      $.q_string,
      $.dollar_quoted_string,
      $.number_literal,
      $.boolean_literal,
      $.null_literal,
      $.typed_literal,
    ),

  line_comment: ($) => token(seq('--', /[^\r\n]*/)),

  // ── Types ────────────────────────────────────────────────────────────────

  _data_type: ($) => choice($.data_type, $.percent_type, $.percent_rowtype),

  // `prec.right`: the trailing modifiers (`WITH TIME ZONE`, `[]`, `ARRAY`) are
  // all optional, so a type has to prefer swallowing them over stopping early.
  data_type: ($) =>
    prec.right(
      seq(
        $._type_name,
        optional($.type_arguments),
        repeat(
          choice(
            kws('WITH', 'LOCAL', 'TIME', 'ZONE'),
            kws('WITH', 'TIME', 'ZONE'),
            kws('WITHOUT', 'TIME', 'ZONE'),
            seq('[', optional($.number_literal), ']'),
            kw('ARRAY'),
            kws('CHARACTER', 'SET'),
          ),
        ),
        ),
    ),

  // Multi-word type names have to be spelled out: `DOUBLE PRECISION` is one
  // type, not a name followed by a name.
  _type_name: ($) =>
    choice(
      kws('DOUBLE', 'PRECISION'),
      // `prec.right` on the two-word forms: VARYING and RAW are unreserved, so
      // without it `CHARACTER VARYING` could stop after the first word and leave
      // the second to be read as something else.
      prec.right(seq(kw('CHARACTER'), optional(kw('VARYING')))),
      prec.right(seq(kw('BIT'), optional(kw('VARYING')))),
      prec.right(seq(kw('LONG'), optional(choice(kw('RAW'), kw('VARCHAR'))))),
      prec.right(seq(kw('INTERVAL'), optional($.interval_qualifier))),
      // Type names that are keywords elsewhere. Admitting them HERE rather than
      // in `_name` is what keeps them out of every other lookahead set.
      kw('DATE'),
      kw('TIME'),
      kw('TIMESTAMP'),
      kw('CHAR'),
      kw('RAW'),
      $.object_name,
    ),

  // `NUMBER(10,2)`, `VARCHAR2(30 CHAR)`, `NUMERIC(*)`.
  type_arguments: ($) =>
    seq(
      '(',
      commaSep1(
        choice(
          seq($.number_literal, optional(choice(kw('BYTE'), kw('CHAR')))),
          $._name,
          '*',
        ),
      ),
      ')',
    ),

  // `prec.right` so `INTERVAL '1' DAY(3)` takes the precision rather than
  // stopping at DAY and leaving `(3)` to whatever comes next.
  interval_qualifier: ($) =>
    prec.right(
      seq(
        $._interval_field,
        optional(seq('(', $.number_literal, ')')),
        optional(seq(kw('TO'), $._interval_field, optional(seq('(', $.number_literal, ')')))),
        ),
    ),

  _interval_field: ($) =>
    choice(kw('YEAR'), kw('MONTH'), kw('DAY'), kw('HOUR'), kw('MINUTE'), kw('SECOND')),

  // Oracle-only: `emp.sal%TYPE`, `emp%ROWTYPE`. Named nodes on purpose — this is
  // one of the constructs the cross-dialect report has to be able to point at.
  // The `%TYPE` glue is a single token, so `a % type` (a modulo of a column
  // called `type`) is left alone.
  percent_type: ($) => seq($.object_name, alias(token(seq('%', ci('TYPE'))), '%TYPE')),

  percent_rowtype: ($) =>
    seq($.object_name, alias(token(seq('%', ci('ROWTYPE'))), '%ROWTYPE')),

  // ── The word token, declared LAST ────────────────────────────────────────
  //
  // After every keyword in this module, and this module is merged after every
  // other one (see `grammar.js`). That position is load-bearing: `identifier`
  // matches every keyword too, and tree-sitter breaks a same-length lexical tie
  // by declaration order. Move these two rules up and `DOUBLE PRECISION`
  // quietly becomes a column named DOUBLE followed by a syntax error.

  // Trailing `$` and `#` are legal in Oracle identifiers (`SYS$SESSION`,
  // `EMP#`), which is why they are accepted here — and why the scanner has to
  // be careful about a `$` that starts a dollar-quoted body.
  identifier: ($) => /[A-Za-z_][A-Za-z0-9_$#]*/,

  // `"…"` with `""` doubling. Case is preserved *and significant*: a quoted
  // name is the one place where Oracle and PostgreSQL agree not to fold.
  quoted_identifier: ($) => /"([^"]|"")*"/,
};
