// grove — Tree-sitter external scanner.
//
// Owns the context-sensitive lexing the context-free grammar can't express
// (see the header comment in `grammar.js` for the division of labour):
//
//   * the island-mode switch — `island_start` matches `s(` / `sound(` / `n(` /
//     `note(` and pushes a Sound/Note mode; `island_end` matches the balancing
//     `)` and pops it. The mode is what lets the same characters lex as a
//     sample name inside `s(...)` and a pitch inside `n(...)`;
//   * the mode-tracked island leaves: `sound_name` (sound mode) and `note_name`
//     (note mode), plus the `chord_name` that follows a `'`;
//   * the host pitch literal `c4` (mandatory octave) — read together with the
//     island-start probe because both begin with a letter and only one lex of
//     the leading word is possible per scan;
//   * the `..` / `..=` / `.` / float maximal munch in host mode.
//
// Bracket balancing (`[ ]`, `< >`, Euclid `( )`) stays in the grammar: the
// closing `)` of an island is the only `)` where `island_end` is in
// `valid_symbols`, so no manual depth counter is needed here.

#include "tree_sitter/parser.h"

#include <stdlib.h>
#include <string.h>
#include <wctype.h>

// Order MUST match the `externals` array in `grammar.js`.
enum TokenType {
  ISLAND_START,
  ISLAND_END,
  SOUND_NAME,
  NOTE_NAME,
  CHORD_NAME,
  NOTE_LITERAL,
  FLOAT,
  INTEGER,
  RANGE_OP,
  RANGE_INCLUSIVE_OP,
  DOT,
  ERROR_SENTINEL,
};

enum Mode {
  MODE_SOUND = 1,
  MODE_NOTE = 2,
};

// Islands cannot nest (a `$` splice takes a bare identifier, not another
// island), so a depth of 1 is the norm; the small stack is pure insurance.
#define STACK_MAX 32
#define WORD_MAX 64

typedef struct {
  unsigned char modes[STACK_MAX];
  unsigned len;
} Scanner;

static inline bool is_digit(int32_t c) { return c >= '0' && c <= '9'; }
static inline bool is_lower(int32_t c) { return c >= 'a' && c <= 'z'; }
static inline bool is_note_letter(int32_t c) { return c >= 'a' && c <= 'g'; }
static inline bool is_ident_start(int32_t c) {
  return c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
}
static inline bool is_ident_cont(int32_t c) {
  return is_ident_start(c) || is_digit(c);
}

static inline void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static inline void skip(TSLexer *lexer) { lexer->advance(lexer, true); }

// `[a-g] (s|f)? [0-9]+` — a host pitch literal, octave mandatory. `word` is the
// already-read identifier run, so the check is purely on its shape.
static bool word_is_note_literal(const char *word, unsigned len) {
  if (len < 2) return false; // at least a letter and one octave digit
  unsigned i = 0;
  if (!is_note_letter((int32_t)word[i])) return false;
  i++;
  if (word[i] == 's' || word[i] == 'f') i++;
  if (i >= len) return false; // no octave digits
  for (; i < len; i++) {
    if (!is_digit((int32_t)word[i])) return false;
  }
  return true;
}

static unsigned char island_fn_mode(const char *word, unsigned len) {
  if (len == 1 && word[0] == 's') return MODE_SOUND;
  if (len == 5 && memcmp(word, "sound", 5) == 0) return MODE_SOUND;
  if (len == 1 && word[0] == 'n') return MODE_NOTE;
  if (len == 4 && memcmp(word, "note", 4) == 0) return MODE_NOTE;
  return 0;
}

// A digit run: `[0-9]+` with an optional `.` fraction. The fraction commits to
// a float only when a digit follows the dot, so `0..8` keeps its `..` for the
// range scanner.
static bool scan_number(TSLexer *lexer, const bool *valid_symbols) {
  while (is_digit(lexer->lookahead)) advance(lexer);
  if (lexer->lookahead == '.' && valid_symbols[FLOAT]) {
    lexer->mark_end(lexer); // integer boundary, in case the fraction aborts
    advance(lexer);         // consume '.'
    if (is_digit(lexer->lookahead)) {
      while (is_digit(lexer->lookahead)) advance(lexer);
      lexer->mark_end(lexer);
      lexer->result_symbol = FLOAT;
      return true;
    }
    // `N..` or `N.x` — the integer is the token; the `.` is left untouched
    // (the earlier mark_end caps the token before it).
    lexer->result_symbol = INTEGER;
    return valid_symbols[INTEGER];
  }
  lexer->mark_end(lexer);
  lexer->result_symbol = INTEGER;
  return valid_symbols[INTEGER];
}

// A letter at host position: either the head of an island (`s(`/`sound(`/…) or a
// pitch literal (`c4`). Both need the leading word, and a scan can only lex it
// once, so they share this probe. On no match we return false and the internal
// lexer re-lexes the word as an `identifier`/keyword.
static bool scan_host_word(Scanner *s, TSLexer *lexer, const bool *valid_symbols) {
  char word[WORD_MAX];
  unsigned len = 0;
  while (is_ident_cont(lexer->lookahead)) {
    if (len < WORD_MAX) word[len] = (char)lexer->lookahead;
    len++;
    advance(lexer);
  }
  if (len > WORD_MAX) return false; // implausibly long: treat as identifier

  if (valid_symbols[ISLAND_START]) {
    unsigned char mode = island_fn_mode(word, len);
    if (mode != 0) {
      while (iswspace(lexer->lookahead)) skip(lexer);
      if (lexer->lookahead == '(') {
        advance(lexer);
        lexer->mark_end(lexer);
        if (s->len < STACK_MAX) s->modes[s->len++] = mode;
        lexer->result_symbol = ISLAND_START;
        return true;
      }
      return false; // `s`/`n`/… not followed by `(` → a plain identifier
    }
  }

  if (valid_symbols[NOTE_LITERAL] && word_is_note_literal(word, len)) {
    lexer->mark_end(lexer);
    lexer->result_symbol = NOTE_LITERAL;
    return true;
  }

  return false;
}

bool tree_sitter_grove_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  Scanner *s = (Scanner *)payload;

  // In error recovery TS marks every external valid; bow out so its internal
  // lexer can resynchronise.
  if (valid_symbols[ERROR_SENTINEL]) return false;

  while (iswspace(lexer->lookahead)) skip(lexer);

  int32_t c = lexer->lookahead;
  unsigned char mode = s->len > 0 ? s->modes[s->len - 1] : 0;

  // 1. Island close — the `)` that balances this island's `island_start`.
  if (mode != 0 && valid_symbols[ISLAND_END] && c == ')') {
    advance(lexer);
    lexer->mark_end(lexer);
    s->len--;
    lexer->result_symbol = ISLAND_END;
    return true;
  }

  // 2. Mode-tracked island leaves.
  if (mode == MODE_SOUND && valid_symbols[SOUND_NAME] && is_lower(c)) {
    advance(lexer); // [a-z][a-z0-9]*
    while (is_lower(lexer->lookahead) || is_digit(lexer->lookahead)) advance(lexer);
    lexer->mark_end(lexer);
    lexer->result_symbol = SOUND_NAME;
    return true;
  }
  if (mode == MODE_NOTE && valid_symbols[NOTE_NAME] && is_note_letter(c)) {
    advance(lexer); // [a-g](s|f)?[0-9]?
    if (lexer->lookahead == 's' || lexer->lookahead == 'f') advance(lexer);
    if (is_digit(lexer->lookahead)) advance(lexer);
    lexer->mark_end(lexer);
    lexer->result_symbol = NOTE_NAME;
    return true;
  }

  // 3. Chord name after a `'` (may be all digits, e.g. `c4'7`), so it precedes
  //    the number scan.
  if (valid_symbols[CHORD_NAME] && (is_ident_start(c) || is_digit(c))) {
    advance(lexer);
    while (is_ident_cont(lexer->lookahead)) advance(lexer);
    lexer->mark_end(lexer);
    lexer->result_symbol = CHORD_NAME;
    return true;
  }

  // 4. Numbers (host integer/float, island degree leaf, postfix count).
  if (is_digit(c) && (valid_symbols[INTEGER] || valid_symbols[FLOAT])) {
    return scan_number(lexer, valid_symbols);
  }

  // 5. Range / method dot — maximal munch (`..=` > `..` > `.`).
  if (c == '.' &&
      (valid_symbols[RANGE_OP] || valid_symbols[RANGE_INCLUSIVE_OP] || valid_symbols[DOT])) {
    advance(lexer);
    if (lexer->lookahead == '.') {
      advance(lexer);
      if (lexer->lookahead == '=') {
        advance(lexer);
        lexer->mark_end(lexer);
        lexer->result_symbol = RANGE_INCLUSIVE_OP;
        return valid_symbols[RANGE_INCLUSIVE_OP];
      }
      lexer->mark_end(lexer);
      lexer->result_symbol = RANGE_OP;
      return valid_symbols[RANGE_OP];
    }
    lexer->mark_end(lexer);
    lexer->result_symbol = DOT;
    return valid_symbols[DOT];
  }

  // 6. A letter at host position → island head or pitch literal.
  if (is_ident_start(c) && (valid_symbols[ISLAND_START] || valid_symbols[NOTE_LITERAL])) {
    return scan_host_word(s, lexer, valid_symbols);
  }

  return false;
}

void *tree_sitter_grove_external_scanner_create(void) {
  return calloc(1, sizeof(Scanner));
}

void tree_sitter_grove_external_scanner_destroy(void *payload) {
  free(payload);
}

unsigned tree_sitter_grove_external_scanner_serialize(void *payload, char *buffer) {
  Scanner *s = (Scanner *)payload;
  unsigned len = s->len > STACK_MAX ? STACK_MAX : s->len;
  if (len) memcpy(buffer, s->modes, len);
  return len;
}

void tree_sitter_grove_external_scanner_deserialize(void *payload, const char *buffer,
                                                    unsigned length) {
  Scanner *s = (Scanner *)payload;
  s->len = length > STACK_MAX ? STACK_MAX : length;
  if (s->len) memcpy(s->modes, buffer, s->len);
}
