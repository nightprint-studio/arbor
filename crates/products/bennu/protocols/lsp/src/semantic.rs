//! Semantic tokens: the server's delta-encoded integer stream → byte spans carrying an
//! editor token class.
//!
//! This is what makes Rust look like Rust. A regex/stream highlighter can only see
//! shapes, so `Foo` is the same colour whether it is a struct, a trait or a type alias,
//! and `println!` is the same colour as a local. rust-analyzer knows which is which,
//! and `textDocument/semanticTokens/full` is how it says so.
//!
//! ## The wire format, and why it needs decoding
//!
//! Tokens arrive as a flat `Vec<u32>`, **five integers each**, each token stated
//! *relative to the one before it*:
//!
//! | # | field | meaning |
//! |---|---|---|
//! | 0 | `deltaLine` | lines since the previous token |
//! | 1 | `deltaStart` | if `deltaLine == 0`, columns since the previous token's start; **otherwise** columns from the start of the line |
//! | 2 | `length` | the token's length |
//! | 3 | `tokenType` | index into the server's `legend.tokenTypes` |
//! | 4 | `tokenModifiers` | **bitset** over the server's `legend.tokenModifiers` |
//!
//! Columns and lengths are in the negotiated [`PositionEncoding`] units, so a line with
//! an accent in it shifts every token after it unless the conversion goes through the
//! line index — which is the whole reason this is decoded in the backend, where the
//! buffer and its index already are, rather than in the editor.
//!
//! Two consequences of the relative encoding worth stating: it must be decoded strictly
//! in order (a token cannot be interpreted alone), and a stream whose length is not a
//! multiple of five is corrupt rather than partially usable.
//!
//! ## Mapping to the editor's vocabulary
//!
//! The token *type* is an index into a legend the server chooses, so the names are the
//! contract, not the numbers. The standard set is small; rust-analyzer extends it
//! heavily (`lifetime`, `macroBang`, `selfKeyword`, `builtinType`, …). Unknown names
//! degrade to a plain identifier rather than being dropped — an unstyled token is
//! invisible, a dropped one leaves a hole in the middle of a coloured line.

use crate::line_index::{LineIndex, PositionEncoding, Position};
use crate::types::SemanticTokensLegend;

/// One highlighted span: a byte range plus the editor classes to paint it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    /// Start byte offset in the file.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The primary token class — one of the editor's `TokenClass` names.
    pub class: String,
    /// Extra modifier names the editor styles on top (`mutable`, `unsafe`, …). Filtered
    /// to the set the theme knows, so the editor never has to guess.
    pub modifiers: Vec<String>,
}

/// Decode a server's semantic-token stream into byte spans.
///
/// `data` is the raw five-ints-per-token array, `legend` the vocabulary from the
/// handshake, `index` an index over the **same text the tokens were computed for**, and
/// `encoding` the negotiated position encoding.
///
/// A malformed stream yields the tokens decoded so far rather than nothing: partial
/// colouring is strictly better than a file that suddenly loses its highlighting because
/// a server appended a stray integer.
pub fn decode(
    data: &[u32],
    legend: &SemanticTokensLegend,
    index: &LineIndex<'_>,
    encoding: PositionEncoding,
) -> Vec<TokenSpan> {
    let mut out = Vec::with_capacity(data.len() / 5);
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for chunk in data.chunks_exact(5) {
        let (delta_line, delta_start, length, type_idx, modifier_bits) =
            (chunk[0], chunk[1], chunk[2], chunk[3], chunk[4]);

        line = line.saturating_add(delta_line);
        // The rule that catches everyone: `deltaStart` is relative to the previous token
        // only while we are still on the same line; a new line resets the origin to the
        // line start.
        character =
            if delta_line == 0 { character.saturating_add(delta_start) } else { delta_start };

        if length == 0 {
            continue; // a zero-length token has nothing to paint
        }

        let start = index.offset_at(Position::new(line, character), encoding);
        let end =
            index.offset_at(Position::new(line, character.saturating_add(length)), encoding);
        if end <= start {
            continue; // clamped away (a token past the end of a stale buffer)
        }

        let type_name = legend.token_types.get(type_idx as usize).map(String::as_str).unwrap_or("");
        let modifiers = decode_modifiers(modifier_bits, legend);
        let class = class_for(type_name, &modifiers);

        out.push(TokenSpan {
            start,
            end,
            class: class.to_string(),
            modifiers: modifiers
                .iter()
                .filter(|m| STYLED_MODIFIERS.contains(&m.as_str()))
                .cloned()
                .collect(),
        });
    }
    out
}

/// The modifier names set in `bits`, by position in the legend.
fn decode_modifiers(bits: u32, legend: &SemanticTokensLegend) -> Vec<String> {
    if bits == 0 {
        return Vec::new();
    }
    legend
        .token_modifiers
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < 32 && bits & (1 << i) != 0)
        .map(|(_, name)| name.clone())
        .collect()
}

/// Modifiers the editor theme actually styles. Everything else is dropped rather than
/// passed through: a class the theme doesn't know is dead weight in the DOM of every
/// token on screen.
const STYLED_MODIFIERS: &[&str] = &["mutable", "unsafe", "deprecated", "async", "documentation"];

/// Map a server token type (plus its modifiers) onto the editor's token-class vocabulary.
///
/// Where the mapping is not one-to-one, the modifiers decide — which is the point of
/// having them: a `function` with `declaration` is the name in `fn foo()`, and colouring
/// it like a call site is what makes a Rust file read as an undifferentiated wall.
fn class_for(type_name: &str, modifiers: &[String]) -> &'static str {
    let has = |m: &str| modifiers.iter().any(|x| x == m);

    // The guarded arms come first, all of them: a `match` takes the first arm that
    // matches, so an unguarded `"property" =>` above a guarded `"property" if …` would
    // make the guard dead code (and earn an `unreachable_patterns` warning).
    match type_name {
        // ── declarations vs uses ───────────────────────────────────────────────
        "function" | "method" if has("declaration") => "declaration",

        // ── a binding the compiler knows is fixed reads as a constant ──────────
        "variable" | "property" | "parameter" if has("constant") || has("static") => "constant",

        // ── types ──────────────────────────────────────────────────────────────
        "type" | "class" | "struct" | "enum" | "interface" | "union" | "typeAlias"
        | "typeParameter" | "builtinType" | "generic" | "trait" => "type",

        // ── callables ──────────────────────────────────────────────────────────
        "function" | "method" => "function",
        // A macro invocation is not a call and not a keyword; it gets its own colour so
        // `println!` stops reading as an ordinary function.
        "macro" | "macroBang" | "derive" => "macro",

        // ── data ───────────────────────────────────────────────────────────────
        "property" | "field" => "field",
        "enumMember" | "constParameter" => "constant",
        "parameter" => "parameter",
        "variable" => "ident",
        "namespace" | "module" | "event" => "ident",

        // ── keywords ───────────────────────────────────────────────────────────
        "selfKeyword" | "selfTypeKeyword" => "self",
        "keyword" | "modifier" | "controlFlow" => "keyword",

        // ── literals ───────────────────────────────────────────────────────────
        "string" | "character" | "regexp" => "string",
        "number" => "number",
        "boolean" | "bool" => "constant",
        // `\n` inside a string, and `{:?}` inside a `format!` — both are code hiding in a
        // literal, and both are much easier to read picked out from it.
        "escapeSequence" | "formatSpecifier" => "escape",
        "invalidEscapeSequence" | "unresolvedReference" => "invalid",

        // ── attributes / lifetimes / labels ────────────────────────────────────
        "attribute" | "builtinAttribute" | "attributeBracket" | "decorator" | "deriveHelper"
        | "toolModule" => "annotation",
        "lifetime" => "lifetime",
        "label" => "label",

        // ── trivia ─────────────────────────────────────────────────────────────
        "comment" => "comment",
        "operator" | "arithmetic" | "logical" | "comparison" | "bitwise" => "operator",
        "punctuation" | "brace" | "bracket" | "parenthesis" | "angle" | "comma" | "semicolon"
        | "colon" | "dot" => "punctuation",

        // An unknown type name is styled as a plain identifier rather than dropped: an
        // unstyled token is invisible, a missing one is a hole in a coloured line.
        _ => "ident",
    }
}

/// The token-class names this module can emit, for the editor theme to cover.
///
/// Exists so the two sides cannot drift silently: the theme is expected to style every
/// name here, and a test in the editor's own suite can assert it does.
pub const EMITTED_CLASSES: &[&str] = &[
    "annotation",
    "comment",
    "constant",
    "declaration",
    "escape",
    "field",
    "function",
    "ident",
    "invalid",
    "keyword",
    "label",
    "lifetime",
    "macro",
    "number",
    "operator",
    "parameter",
    "punctuation",
    "self",
    "string",
    "type",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn legend() -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: ["function", "struct", "variable", "comment", "macro", "lifetime"]
                .map(String::from)
                .to_vec(),
            token_modifiers: ["declaration", "mutable", "static", "unsafe"]
                .map(String::from)
                .to_vec(),
        }
    }

    #[test]
    fn a_single_token_decodes_to_its_byte_span() {
        let text = "fn main() {}";
        let idx = LineIndex::new(text);
        // line 0, char 3, len 4, type 0 (function), modifier bit 0 (declaration)
        let spans = decode(&[0, 3, 4, 0, 0b0001], &legend(), &idx, PositionEncoding::Utf16);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].start..spans[0].end], "main");
        assert_eq!(spans[0].class, "declaration", "a declared fn name is not a call site");
    }

    #[test]
    fn delta_start_is_relative_within_a_line_and_absolute_after_a_newline() {
        // The rule the format is famous for getting wrong. Two tokens on line 0 then one
        // on line 1: the third's `deltaStart` is measured from the line start, NOT from
        // the second token.
        let text = "aa bb\ncc";
        let idx = LineIndex::new(text);
        let data = [
            0, 0, 2, 2, 0, // "aa" at 0:0
            0, 3, 2, 2, 0, // "bb" at 0:3 (delta 3 from "aa" start)
            1, 0, 2, 2, 0, // "cc" at 1:0 — absolute, not 0+3+…
        ];
        let spans = decode(&data, &legend(), &idx, PositionEncoding::Utf16);
        let texts: Vec<&str> = spans.iter().map(|s| &text[s.start..s.end]).collect();
        assert_eq!(texts, vec!["aa", "bb", "cc"]);
    }

    #[test]
    fn columns_are_in_the_negotiated_encoding_not_bytes() {
        // A non-ASCII char earlier on the line shifts every following token unless the
        // conversion goes through the line index. `à` is 2 bytes, 1 UTF-16 unit.
        let text = "let città = 1;";
        let idx = LineIndex::new(text);
        // The `1` literal: UTF-16 column 12, byte offset 13.
        let spans = decode(&[0, 12, 1, 2, 0], &legend(), &idx, PositionEncoding::Utf16);
        assert_eq!(&text[spans[0].start..spans[0].end], "1", "shifted by the accent");
        // Told the same thing in bytes, the server would have said 13.
        let spans = decode(&[0, 13, 1, 2, 0], &legend(), &idx, PositionEncoding::Utf8);
        assert_eq!(&text[spans[0].start..spans[0].end], "1");
    }

    #[test]
    fn modifiers_are_decoded_from_the_bitset_and_filtered_to_the_styled_set() {
        let idx = LineIndex::new("let mut x = 1;");
        // type 2 (variable), modifiers: bit 1 = mutable, bit 2 = static
        let spans = decode(&[0, 8, 1, 2, 0b0110], &legend(), &idx, PositionEncoding::Utf16);
        assert_eq!(spans[0].class, "constant", "`static` promotes a variable to a constant");
        assert_eq!(spans[0].modifiers, vec!["mutable"], "`static` is not a styled modifier");
    }

    #[test]
    fn a_stream_that_is_not_a_multiple_of_five_keeps_what_it_could_decode() {
        let idx = LineIndex::new("ab cd");
        let data = [0, 0, 2, 2, 0, /* a stray tail */ 0, 3];
        let spans = decode(&data, &legend(), &idx, PositionEncoding::Utf16);
        assert_eq!(spans.len(), 1, "partial colouring beats no colouring");
    }

    #[test]
    fn a_zero_length_token_is_skipped() {
        let idx = LineIndex::new("abc");
        assert!(decode(&[0, 0, 0, 2, 0], &legend(), &idx, PositionEncoding::Utf16).is_empty());
    }

    #[test]
    fn tokens_past_the_end_of_a_stale_buffer_are_dropped_not_clamped_to_nothing() {
        // The tokens were computed for a longer version of the file. Every span that
        // clamps to an empty range is dropped rather than emitted as a zero-width mark.
        let idx = LineIndex::new("ab");
        let spans = decode(&[9, 0, 4, 2, 0], &legend(), &idx, PositionEncoding::Utf16);
        assert!(spans.is_empty());
    }

    #[test]
    fn an_unknown_token_type_degrades_to_an_identifier() {
        let idx = LineIndex::new("abc");
        // type index 99 is not in the legend.
        let spans = decode(&[0, 0, 3, 99, 0], &legend(), &idx, PositionEncoding::Utf16);
        assert_eq!(spans[0].class, "ident", "unstyled beats a hole in the line");
    }

    #[test]
    fn a_modifier_bit_past_the_legend_is_ignored() {
        let idx = LineIndex::new("abc");
        let spans = decode(&[0, 0, 3, 2, 1 << 20], &legend(), &idx, PositionEncoding::Utf16);
        assert!(spans[0].modifiers.is_empty());
    }

    #[test]
    fn the_rust_vocabulary_lands_on_distinct_classes() {
        // The point of the whole feature: these must not collapse into one colour.
        let cases = [
            ("struct", "type"),
            ("trait", "type"),
            ("typeAlias", "type"),
            ("builtinType", "type"),
            ("macro", "macro"),
            ("macroBang", "macro"),
            ("lifetime", "lifetime"),
            ("selfKeyword", "self"),
            ("parameter", "parameter"),
            ("property", "field"),
            ("enumMember", "constant"),
            ("escapeSequence", "escape"),
            ("formatSpecifier", "escape"),
            ("unresolvedReference", "invalid"),
            ("builtinAttribute", "annotation"),
            ("comment", "comment"),
            ("bool", "constant"),
            ("character", "string"),
            ("arithmetic", "operator"),
            ("brace", "punctuation"),
        ];
        for (type_name, expected) in cases {
            assert_eq!(class_for(type_name, &[]), expected, "{type_name}");
        }
    }

    #[test]
    fn a_declaration_modifier_only_promotes_callables() {
        let decl = vec!["declaration".to_string()];
        assert_eq!(class_for("function", &decl), "declaration");
        assert_eq!(class_for("method", &decl), "declaration");
        // A declared struct is still a type — `declaration` here would lose the type colour.
        assert_eq!(class_for("struct", &decl), "type");
        assert_eq!(class_for("variable", &decl), "ident");
    }

    #[test]
    fn every_class_the_mapper_can_emit_is_declared() {
        // The list the editor theme is expected to cover. If a new mapping arm appears
        // without an entry here, the token silently renders unstyled.
        let probe = [
            "function", "method", "struct", "trait", "macro", "lifetime", "selfKeyword",
            "parameter", "property", "enumMember", "escapeSequence", "unresolvedReference",
            "attribute", "comment", "operator", "brace", "keyword", "string", "number",
            "variable", "namespace", "label", "bool", "somethingUnknown",
        ];
        for type_name in probe {
            for mods in [vec![], vec!["declaration".to_string()], vec!["constant".to_string()]] {
                let c = class_for(type_name, &mods);
                assert!(EMITTED_CLASSES.contains(&c), "{type_name}/{mods:?} → {c} is not declared");
            }
        }
    }
}
