// Case-insensitive keyword tokens.
//
// SQL keywords are case-insensitive and real scripts mix `SELECT` with `select`
// freely, so every keyword is a character-class regex aliased back to its
// canonical upper-case spelling. Aliasing to a plain string keeps them
// *anonymous* nodes: they never show up in corpus tests or in the Rust walker,
// which is what makes the trees readable.
//
// The objects are cached because tree-sitter identifies tokens by object
// identity — building `kw('SELECT')` twice would otherwise define two tokens
// with the same regex and provoke a lexical conflict.

const CACHE = {};

/** `SELECT` → `/[Ss][Ee][Ll][Ee][Cc][Tt]/` */
function ci(word) {
  return new RegExp(
    word
      .split('')
      .map((c) => (/[a-zA-Z]/.test(c) ? `[${c.toLowerCase()}${c.toUpperCase()}]` : c))
      .join(''),
  );
}

/** One case-insensitive keyword token, named after its upper-case spelling. */
function kw(word) {
  if (!CACHE[word]) CACHE[word] = alias(token(ci(word)), word);
  return CACHE[word];
}

/** `kws('GROUP', 'BY')` — a keyword phrase. */
function kws(...words) {
  return seq(...words.map(kw));
}

// Keywords that are ALSO ordinary identifiers.
//
// `word: $ => $.identifier` turns on tree-sitter's keyword extraction, and the
// price is that a keyword can no longer be lexed as a name. That is fatal for
// legacy schemas, where columns called VALUE, TYPE, LEVEL, STATE or SIZE are
// everywhere. Each word listed here is re-admitted as an `identifier` by the
// `_name` rule, so `PARAMETRI.VALUE` parses while `VALUES (…)` still works.
//
// A word only belongs here if it can never start a construct where a bare name
// is also legal — that is why FROM, WHERE, SELECT, JOIN, VALUES … are absent:
// they are exactly the fences that let an unquoted alias stop at the right byte.
// The list is kept DELIBERATELY SHORT — every entry costs a token that is valid
// wherever a name is, and the generated parse table grows with the product of
// states and valid tokens. Words that are not keywords anywhere in this grammar
// (`VALUE`, `NAME`, `SIZE`, `LEVEL`, `STATE`, `VERSION`, `TEXT`, `COUNT`, …) do
// NOT belong here: they are already plain identifiers.
//
// Type names that double as keywords (`DATE`, `TIMESTAMP`, `CHAR`, `RAW`,
// `PRECISION`) are handled in `_type_name` instead, which admits them where a
// type is expected without making them names everywhere.
const UNRESERVED = ['ACTION', 'DATA', 'KEY', 'LANGUAGE', 'MATCH', 'REPLACE', 'ROLE', 'TYPE'];

module.exports = { ci, kw, kws, UNRESERVED };
