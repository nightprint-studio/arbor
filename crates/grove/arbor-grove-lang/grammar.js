/**
 * grove — Tree-sitter grammar (Standard level).
 *
 * Mirrors `design/grove/grammar.md` and the typed AST in `src/ast.rs`
 * one-to-one: every named node here maps onto exactly one AST variant so the
 * CST→AST walker (`src/parse.rs`) stays mechanical.
 *
 * Division of labour with the external scanner (`src/scanner.c`):
 *   - The scanner owns the **context-sensitive lexing** the CFG can't express:
 *     the island-mode switch (`island_start` = `s(`/`sound(`/`n(`/`note(`, which
 *     also tells the walker the island kind; `island_end` = the matching `)`),
 *     the mode-tracked island leaves (`sound_name` in sound mode, `note_name` in
 *     note mode), the chord name after `'`, the host pitch literal `c4`
 *     (mandatory octave, so it never collides with `identifier`), and the
 *     `..` / `..=` / `.` / float maximal munch.
 *   - The CFG owns the **balancing**: `[ ]` groups and `< >` alternations nest
 *     through ordinary rules, and Euclid `( … )` is a plain parenthesised
 *     postfix — so the scanner needs no manual bracket counter.
 *
 * Precedence ladder (host): lambda < range < add < mul < unary < postfix.
 * Inside islands: `&` (parallel) < juxtaposition (sequence) < postfixes.
 */

const PREC = {
  lambda: 1,
  range: 2,
  add: 3,
  mul: 4,
  unary: 5,
  postfix: 6,
};

module.exports = grammar({
  name: 'grove',

  // Order MUST match the `TokenType` enum in `src/scanner.c`.
  externals: $ => [
    $.island_start,         // `s(` `sound(` `n(` `note(`  — pushes island mode
    $.island_end,           // the `)` that closes an island — pops island mode
    $.sound_name,           // island sound leaf  (sound mode): [a-z][a-z0-9]*
    $.note_name,            // island note leaf   (note mode):  [a-g](s|f)?[0-9]?
    $.chord_name,           // island chord after `'`
    $.note_literal,         // host pitch literal: [a-g](s|f)?[0-9]+ (octave required)
    $.float,                // [0-9]+ '.' [0-9]+
    $.integer,              // [0-9]+   (host int, island degree leaf, postfix count)
    $.range_op,             // '..'
    $.range_inclusive_op,   // '..='
    $.dot,                  // '.'  (method chain)
    $._error_sentinel,      // never emitted — lets the scanner detect error recovery
  ],

  // Whitespace and comments are skippable everywhere. Inside islands the
  // "space = sequence" rule is modelled as juxtaposition (`repeat1` of terms),
  // so whitespace carries no meaning of its own and is safe to treat as extra.
  extras: $ => [/\s/, $.comment],

  word: $ => $.identifier,

  conflicts: $ => [
    // `( ident )` is ambiguous between a parenthesised expression `( expr )`
    // and a single-parameter lambda head `( params ) =>`. GLR keeps both until
    // the `=>` (or its absence) decides.
    [$._expression, $.parameters],
  ],

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.import_statement,
      $.let_binding,
      $.fn_definition,
      $._expression, // a bare top-level expression: the output
    ),

    comment: _ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),

    // ── Statements ────────────────────────────────────────────────────────────
    import_statement: $ => seq(
      'import', '{',
      commaSep1(field('name', $.identifier)),
      '}', 'from',
      field('path', $.string),
    ),

    let_binding: $ => seq(
      'let', field('name', $.identifier), '=', field('value', $._expression),
    ),

    fn_definition: $ => seq(
      'fn', field('name', $.identifier),
      '(', optional(field('params', $.parameters)), ')',
      '=', field('body', $._expression),
    ),

    parameters: $ => commaSep1($.identifier),

    // ── Expressions ──────────────────────────────────────────────────────────
    _expression: $ => choice(
      $.lambda,
      $.range_expression,
      $.binary_expression,
      $.unary_expression,
      $.method_call,
      $.call_expression,
      $.island,
      $.parenthesized,
      $.note_literal,
      $.identifier,
      $.number,
      $.string,
    ),

    lambda: $ => prec.right(PREC.lambda, seq(
      field('params', choice($.identifier, $._lambda_params)),
      '=>',
      field('body', $._expression),
    )),
    // `(p1, p2) =>` — the parenthesised multi-parameter head.
    _lambda_params: $ => seq('(', optional($.parameters), ')'),

    range_expression: $ => prec.left(PREC.range, seq(
      field('lo', $._expression),
      field('operator', choice($.range_op, $.range_inclusive_op)),
      field('hi', $._expression),
    )),

    binary_expression: $ => choice(
      prec.left(PREC.add, seq(
        field('left', $._expression),
        field('operator', choice('+', '-')),
        field('right', $._expression),
      )),
      prec.left(PREC.mul, seq(
        field('left', $._expression),
        field('operator', choice('*', '/')),
        field('right', $._expression),
      )),
    ),

    unary_expression: $ => prec.right(PREC.unary, seq(
      field('operator', '-'),
      field('operand', $._expression),
    )),

    call_expression: $ => prec(PREC.postfix, seq(
      field('function', $.identifier),
      field('arguments', $.arguments),
    )),

    method_call: $ => prec.left(PREC.postfix, seq(
      field('receiver', $._expression),
      $.dot,
      field('method', $.identifier),
      field('arguments', $.arguments),
    )),

    // A trailing comma is allowed: the emitter prints one in the multi-line
    // `tracks(...)` / `arrange(...)` form, so the canonical output must re-parse.
    arguments: $ => seq(
      '(',
      optional(seq(commaSep1($._expression), optional(','))),
      ')',
    ),

    parenthesized: $ => seq('(', $._expression, ')'),

    number: $ => choice($.integer, $.float),

    // ── Islands (mini-notation) ──────────────────────────────────────────────
    // `island_start` / `island_end` are the scanner's mode brackets; the body
    // in between is ordinary CFG. The island kind (Sound vs Note) is read off
    // the `island_start` token text by the walker.
    island: $ => seq(
      field('open', $.island_start),
      field('body', $._mini),
      field('close', $.island_end),
    ),

    _mini: $ => choice($.parallel, $._sequence),

    // `a & b & c` — only a node when there is at least one `&` (≥ 2 lanes),
    // matching `MiniKind::Parallel`'s "≥ 2 lanes" invariant.
    parallel: $ => prec.left(seq(
      $._sequence, repeat1(seq('&', $._sequence)),
    )),

    _sequence: $ => choice($.sequence, $._term),

    // Juxtaposition — only a node when there are ≥ 2 terms.
    sequence: $ => prec.left(seq($._term, repeat1($._term))),

    _term: $ => choice($.term, $._atom),

    // An atom plus its postfix chain — only a node when there is ≥ 1 postfix.
    term: $ => seq(
      field('atom', $._atom),
      field('postfix', repeat1($._postfix)),
    ),

    _atom: $ => choice(
      $.sound_name,
      $.note_name,
      $.integer,        // a bare degree leaf (note island; eval validates context)
      $.group,
      $.alternation,
      $.polymeter,
      $.rest,
      $.extend,
      $.splice,
    ),

    group: $ => seq('[', $._mini, ']'),
    alternation: $ => seq('<', $._mini, '>'),
    // `{a b c}%n` — polymeter. `%n` sets steps-per-cycle; omitted, the steps
    // default (eval) to the length of the first lane (Strudel semantics).
    polymeter: $ => seq(
      '{', field('body', $._mini), '}',
      optional(seq('%', field('steps', $.integer))),
    ),
    rest: _ => '~',
    extend: _ => '_',
    splice: $ => seq('$', field('name', $.identifier)),

    _postfix: $ => choice(
      $.fast, $.slow, $.replicate, $.weight, $.euclid, $.variant, $.chord,
    ),

    // `*`/`/` factors may be a literal **or** a sub-pattern (`bd*<2 3>`,
    // `bd*[2 3]`, `bd*{2 3}`): a patternised factor that varies per slot/cycle.
    fast: $ => seq('*', field('n', $._factor)),
    slow: $ => seq('/', field('n', $._factor)),
    replicate: $ => seq('!', field('n', $.integer)),
    weight: $ => seq('@', field('n', $.integer)),
    // Each euclid argument may itself be patternised (`bd(<3 5>,8)`).
    euclid: $ => seq(
      '(',
      field('pulses', $._count), ',',
      field('steps', $._count),
      optional(seq(',', field('rotation', $._count))),
      ')',
    ),
    variant: $ => seq(':', field('n', $.integer)),
    chord: $ => seq('\'', field('name', $.chord_name)),

    // A postfix-argument factor: a literal number or a sub-pattern atom.
    _factor: $ => choice($.number, $.alternation, $.group, $.polymeter),
    // A euclid count: a literal integer or a sub-pattern atom.
    _count: $ => choice($.integer, $.alternation, $.group, $.polymeter),

    // ── Leaf tokens ──────────────────────────────────────────────────────────
    identifier: _ => /[a-zA-Z_][a-zA-Z0-9_]*/,
    string: _ => token(seq(
      '"',
      repeat(choice(/[^"\\]/, seq('\\', /./))),
      '"',
    )),
  },
});

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
