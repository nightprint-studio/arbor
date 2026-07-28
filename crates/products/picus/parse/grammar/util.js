// Tiny combinators shared by every grammar module.
//
// The tree-sitter CLI injects its DSL (`seq`, `choice`, `repeat`, …) as globals
// before requiring `grammar.js`, so required modules see them too — that is what
// makes splitting the grammar across files possible at all.

/** `rule (, rule)*` */
function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}

/** `(rule (, rule)*)?` */
function commaSep(rule) {
  return optional(commaSep1(rule));
}

module.exports = { commaSep, commaSep1 };
