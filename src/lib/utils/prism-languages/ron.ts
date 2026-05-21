/**
 * Prism grammar for Rusty Object Notation (RON).
 *
 * RON is lexically close to a subset of Rust (identifiers, named fields,
 * tuple/struct/enum-variant syntax, raw strings, char literals, both line
 * and block comments, numbers with exponents, true/false). Rather than
 * roll a hand-written grammar and risk esbuild choking on inline regex
 * literals, we extend the existing prism-rust grammar — that one is
 * already battle-tested and registered globally via prism-shared.ts.
 *
 * We only override the `keyword` rule so RON's `Some`/`None` get tagged
 * alongside `true`/`false`. Everything else (strings, chars, comments,
 * numbers, types, punctuation) is inherited from Rust as-is.
 */

import Prism from 'prismjs';

// Make absolutely sure prism-rust has loaded before we extend it. The
// import is side-effect-only; the bundler dedupes it with the one in
// prism-shared.ts so there's no runtime cost.
import 'prismjs/components/prism-rust';

Prism.languages.ron = Prism.languages.extend('rust', {
  keyword: /\b(?:true|false|Some|None)\b/,
});
