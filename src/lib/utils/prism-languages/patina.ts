/**
 * Prism grammar for Patina (`.ptn`), the Voriz mod language.
 *
 * A markdown fence in the rendered view is tokenised by Prism, not by CodeMirror, so this mirrors the token
 * classes of the stream mode in `bennu/patina-lang.ts` (calls, fields, associated functions, constants,
 * declarations, module paths, forms, labels, attributes, numbers with their suffixes). Colour only: nothing
 * here knows which names exist, exactly like the stream mode.
 *
 * Registered under `patina` and `ptn`.
 */

import Prism from 'prismjs';

const KEYWORDS =
  'as|break|const|continue|crate|else|enum|fn|for|if|impl|in|let|loop|match|mut|pub|return|super|struct|' +
  'trait|use|where|while|async|await|dyn|move|ref|static|task|type|unsafe|yield|self|Self';

const grammar: Prism.Grammar = {
  comment: /\/\/.*/,
  // An attribute, whole: `#[on(StepOn, block: "landmine")]`.
  'attr-name': /#!?\[[^\]]*\]?/,
  string: { pattern: /"(?:[^"\\\n]|\\.)*"?/, greedy: true },
  // A label: `'outer`. Patina has no character literals.
  symbol: /'[A-Za-z_]\w*/,
  number: /\b(?:0x[0-9a-fA-F_]+|0b[01_]+|\d[\d_]*(?:\.\d[\d_]*)?)(?:f32|min|ms|s|t)?\b/,
  // The names a declaration introduces. They come before `keyword`: the lookbehind needs `fn` still in the text.
  function: { pattern: /(\bfn\s+)[\p{L}_][\p{L}\p{N}_]*/u, lookbehind: true },
  'class-name': [
    { pattern: /(\b(?:struct|enum|trait)\s+)[\p{L}_][\p{L}\p{N}_]*/u, lookbehind: true },
  ],
  // A primitive type, with a class of its own that `app.css` colours with `--syntax-primitive` like the editor does.
  // A name followed by a single colon is a field, not a type.
  'primitive-type': /\b(?:int|fx|f32|bool|str)\b(?!\s*:(?!:))/,
  // A form: `block!("tnt")`, `t!("mod:key")`.
  builtin: /\b[\p{L}_][\p{L}\p{N}_]*(?=!(?!=))/u,
  keyword: new RegExp(`\\b(?:${KEYWORDS})\\b`),
  boolean: /\b(?:true|false)\b/,
  // A method call, an associated function (`Type::new(`) or a plain call. After `keyword`: `if (` is not a call.
  'function-call': { pattern: /\b[\p{L}_][\p{L}\p{N}_]*(?=\s*\()/u, alias: 'function' },
  // A module path segment: `voriz::prelude::`.
  namespace: /\b[a-z_][a-z0-9_]*(?=::)/,
  // A field after a dot.
  property: { pattern: /(\.)[\p{L}_][\p{L}\p{N}_]*/u, lookbehind: true },
  constant: /\b(?:Some|None|Ok|Err|[A-Z][A-Z0-9_]+)\b|(?<=::)[A-Za-z_]\w*/,
  'class-name-type': { pattern: /\b\p{Lu}[\p{L}\p{N}_]*/u, alias: 'class-name' },
  operator: /->|=>|==|!=|<=|>=|&&|\|\||\.\.=?|[-+*/%]=?|[=<>!&|^?]/,
  punctuation: /::|[{}[\]();,.:]/,
};

Prism.languages.patina = grammar;
Prism.languages.ptn = grammar;
