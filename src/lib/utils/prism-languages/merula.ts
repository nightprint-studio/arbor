/**
 * Prism grammar for **`.merula`** — the live-coding language of Arbor's DAW.
 *
 * The editor highlights a merula buffer from its tree-sitter grammar; a fenced ```merula block
 * in a markdown document has no tree, so it needs this. Same relationship `dig.ts` has to the
 * `.dig` grammar, and the same caveat: lexical, approximate, colour only.
 *
 * What the mini-notation needs beyond an ordinary C-family grammar is the reason this file is
 * not three lines of `extend('clike')`: a merula line is mostly `~`, `_`, `[ ]`, `<>`, `*2`,
 * `$name` and note names — punctuation that means something, and that reads as noise when it is
 * all one colour. The classes mirror `merula-lang.ts`'s own `classifyToken` so a fence and the
 * DAW's editor agree about what is a note and what is an operator.
 */

import Prism from 'prismjs';

Prism.languages.merula = {
  comment: { pattern: /\/\/.*|\/\*[\s\S]*?\*\//, greedy: true },
  string: { pattern: /"(?:\\.|[^"\\\r\n])*"/, greedy: true },
  // `$pattern` — a splice: the one reference form the language has, and the thing you look for
  // first when reading somebody else's line.
  variable: /\$[A-Za-z_]\w*/,
  function: [
    { pattern: /\b(?:let|fn)\s+[A-Za-z_]\w*/, inside: { keyword: /^\w+/ } },
    { pattern: /\b[A-Za-z_]\w*(?=\s*\()/ },
    // A method in a chain — `.fast(2)`, `.rev()` — where the dot is the operator and the name
    // is the call.
    { pattern: /(\.)[A-Za-z_]\w*(?=\s*\()/, lookbehind: true },
  ],
  keyword: /\b(?:let|fn|import|from)\b/,
  // A note name with an optional accidental and octave (`c`, `f#3`, `eb2`), and the two
  // mini-notation atoms that are not notes: `~` a rest, `_` an extension of the one before it.
  symbol: /\b[a-g](?:#|b)?\d?\b|[~_]/,
  number: /\b\d+(?:\.\d+)?\b/,
  // `..=` before `..`, or an inclusive range reads as a range followed by an `=`.
  operator: /\.\.=|\.\.|=>|[&*/!@:'%+\-=<>]/,
  punctuation: /[[\](){},]/,
};
