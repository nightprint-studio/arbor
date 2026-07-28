// picus_sql — Tree-sitter external scanner.
//
// Four tokens that no context-free rule can express, and the reason each one
// has to live here:
//
//   * `block_comment` — PostgreSQL nests `/* … /* … */ … */`. A regex ends the
//     comment at the FIRST `*/`, which silently turns the tail of a commented-
//     out block into live SQL. Depth counting needs a loop.
//   * `q_string` — Oracle `q'[…]'`, `q'{…}'`, `q'(…)'`, `q'<…>'`, `q'!…!'`: the
//     closing delimiter is chosen by the opening one, mirrored for the four
//     bracket pairs. The form is only taken when a delimiter follows the quote
//     IMMEDIATELY, so PostgreSQL's `q 'abc'` (a name and a string, or a
//     type-prefixed literal) is left alone.
//   * `dollar_quoted_string` — `$tag$ … $tag$`: the terminator is whatever the
//     opener said it was, so it cannot be a token either. `$1` is not a dollar
//     quote and must fall through to the bind-parameter token.
//   * `slash_terminator` — Oracle's lone `/` on its own line. It is only a
//     terminator when nothing but blanks precede it on its line and nothing but
//     blanks follow it to the newline; `a / b` stays a division, and so does a
//     `/ b` that merely starts a line.
//
// The scanner keeps no state between scans (nesting and tag matching are both
// resolved inside a single token), so serialize/deserialize are empty and a
// resumed parse can never see a stale mode.

#include "tree_sitter/parser.h"

#include <stdlib.h>
#include <string.h>
#include <wctype.h>

// Order MUST match the `externals` array in `grammar.js`.
enum TokenType {
  BLOCK_COMMENT,
  Q_STRING,
  DOLLAR_QUOTED_STRING,
  SLASH_TERMINATOR,
  ERROR_SENTINEL,
};

// A dollar-quote tag is an identifier; anything longer than this is not a tag
// anybody wrote on purpose, and refusing it keeps the buffer fixed-size.
#define TAG_MAX 64

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

static inline bool is_letter(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
}
static inline bool is_digit(int32_t c) { return c >= '0' && c <= '9'; }

static inline bool is_tag_start(int32_t c) { return is_letter(c) || c == '_'; }
static inline bool is_tag_cont(int32_t c) { return is_tag_start(c) || is_digit(c); }

// The four bracket pairs close with their mirror; every other delimiter closes
// with itself (`q'!…!'`).
static int32_t mirrored(int32_t open) {
  switch (open) {
    case '[': return ']';
    case '{': return '}';
    case '(': return ')';
    case '<': return '>';
    default: return open;
  }
}

// `/*` already consumed. Consumes up to and including the balancing `*/`.
static bool scan_block_comment(TSLexer *lexer) {
  unsigned depth = 1;
  for (;;) {
    if (lexer->eof(lexer)) {
      // Unterminated. Refusing the token (rather than swallowing the rest of
      // the file) is what makes a truncated script report an error instead of
      // quietly losing everything after the `/*`.
      return false;
    }
    if (lexer->lookahead == '*') {
      advance(lexer);
      if (lexer->lookahead == '/') {
        advance(lexer);
        if (--depth == 0) break;
      }
    } else if (lexer->lookahead == '/') {
      advance(lexer);
      if (lexer->lookahead == '*') {
        advance(lexer);
        depth++;
      }
    } else {
      advance(lexer);
    }
  }
  lexer->mark_end(lexer);
  lexer->result_symbol = BLOCK_COMMENT;
  return true;
}

// Lookahead is `/`. Handles both tokens that can start with it.
static bool scan_slash(TSLexer *lexer, const bool *valid_symbols, bool at_line_start) {
  advance(lexer);

  if (lexer->lookahead == '*') {
    if (!valid_symbols[BLOCK_COMMENT]) return false;
    advance(lexer);
    return scan_block_comment(lexer);
  }

  if (!valid_symbols[SLASH_TERMINATOR] || !at_line_start) return false;

  // The token is the `/` alone; the trailing blanks are only inspected.
  lexer->mark_end(lexer);
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' || lexer->lookahead == '\r') {
    advance(lexer);
  }
  if (lexer->lookahead == '\n' || lexer->eof(lexer)) {
    lexer->result_symbol = SLASH_TERMINATOR;
    return true;
  }
  return false;
}

// Lookahead is `q` or `Q`.
static bool scan_q_string(TSLexer *lexer) {
  advance(lexer);
  if (lexer->lookahead != '\'') return false;
  advance(lexer);

  int32_t open = lexer->lookahead;
  // Oracle forbids whitespace as the delimiter, and end-of-input is obviously
  // not one. This is the check that keeps `q 'abc'` out.
  if (lexer->eof(lexer) || open == '\'' || iswspace(open)) return false;
  int32_t close = mirrored(open);
  advance(lexer);

  for (;;) {
    if (lexer->eof(lexer)) return false;
    if (lexer->lookahead == close) {
      advance(lexer);
      if (lexer->lookahead == '\'') {
        advance(lexer);
        break;
      }
      // A lone delimiter char inside the body: ordinary content.
    } else {
      advance(lexer);
    }
  }
  lexer->mark_end(lexer);
  lexer->result_symbol = Q_STRING;
  return true;
}

// Lookahead is `$`.
static bool scan_dollar_quoted(TSLexer *lexer) {
  char tag[TAG_MAX];
  unsigned tag_len = 0;

  advance(lexer);
  if (is_tag_start(lexer->lookahead)) {
    while (is_tag_cont(lexer->lookahead)) {
      if (tag_len >= TAG_MAX) return false;
      tag[tag_len++] = (char)lexer->lookahead;
      advance(lexer);
    }
  }
  // `$1` (a positional parameter) and `$` alone stop here.
  if (lexer->lookahead != '$') return false;
  advance(lexer);

  for (;;) {
    if (lexer->eof(lexer)) return false;
    if (lexer->lookahead != '$') {
      advance(lexer);
      continue;
    }
    advance(lexer);
    unsigned matched = 0;
    while (matched < tag_len && lexer->lookahead == (int32_t)(unsigned char)tag[matched]) {
      advance(lexer);
      matched++;
    }
    if (matched == tag_len && lexer->lookahead == '$') {
      advance(lexer);
      break;
    }
    // A partial match is body text; the loop resumes from wherever it stopped,
    // which is why `$$x$` inside a `$x$…$x$` body still terminates correctly.
  }
  lexer->mark_end(lexer);
  lexer->result_symbol = DOLLAR_QUOTED_STRING;
  return true;
}

bool tree_sitter_picus_sql_external_scanner_scan(void *payload, TSLexer *lexer,
                                                 const bool *valid_symbols) {
  (void)payload;

  // During error recovery tree-sitter marks every external valid; bow out so
  // its internal lexer can resynchronise.
  if (valid_symbols[ERROR_SENTINEL]) return false;

  bool saw_newline = false;
  while (iswspace(lexer->lookahead)) {
    if (lexer->lookahead == '\n') saw_newline = true;
    skip(lexer);
  }

  int32_t c = lexer->lookahead;

  if (c == '/') {
    // `get_column` has to be read before anything is consumed. Column 0 covers
    // a `/` at the very start of the file, where no newline was skipped.
    bool at_line_start = saw_newline || lexer->get_column(lexer) == 0;
    return scan_slash(lexer, valid_symbols, at_line_start);
  }

  if (valid_symbols[Q_STRING] && (c == 'q' || c == 'Q')) {
    return scan_q_string(lexer);
  }

  if (valid_symbols[DOLLAR_QUOTED_STRING] && c == '$') {
    return scan_dollar_quoted(lexer);
  }

  return false;
}

void *tree_sitter_picus_sql_external_scanner_create(void) { return calloc(1, 1); }

void tree_sitter_picus_sql_external_scanner_destroy(void *payload) { free(payload); }

unsigned tree_sitter_picus_sql_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_picus_sql_external_scanner_deserialize(void *payload, const char *buffer,
                                                        unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}
