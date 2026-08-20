//! What is declared in a shader — read from the text, not from the compiler.
//!
//! ## Why not ask naga
//!
//! Because the moment this is wanted is the moment naga has nothing to say. Completion
//! fires while you are half way through a line; the outline is read while a brace is still
//! missing; a Bevy shader full of `#import` never parses standalone at all. A scanner that
//! keeps working through all three is worth more than a parse tree that is correct on the
//! files nobody needs help with.
//!
//! So this is deliberately **shallow and tolerant**: it finds declarations by their leading
//! keyword and does not attempt to understand types, scopes or expressions. Everything it
//! reports is really there; it does not claim to report everything.
//!
//! Comments are blanked out before scanning — replaced with spaces of the same length, so
//! every offset this returns is an offset into the original source and no caller has to map
//! anything back.

/// What kind of thing a declaration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgslSymbolKind {
    /// A plain function.
    Function,
    /// A function carrying `@vertex`, `@fragment` or `@compute` — where the GPU starts,
    /// and the first thing anybody looks for in a shader they did not write.
    EntryPoint,
    Struct,
    /// A member of a struct.
    Field,
    Alias,
    Const,
    Override,
    /// A module-scope `var`. Carries its address space in `detail`.
    Var,
}

/// One declaration, located by the **UTF-8 byte offsets of its name** — so a go-to lands on
/// the name rather than on the keyword in front of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslSymbol {
    pub name: String,
    pub kind: WgslSymbolKind,
    pub start: usize,
    pub end: usize,
    /// What to show beside the name: the address space of a `var`, the binding it sits at,
    /// the stage of an entry point.
    pub detail: String,
    /// The struct a field belongs to.
    pub container: Option<String>,
}

/// Comments replaced by spaces, so offsets survive.
///
/// WGSL block comments **nest** (`/* /* */ */` is one comment), which is why this counts a
/// depth rather than looking for the first `*/` — getting that wrong swallows the rest of
/// the file, or ends the comment halfway through it.
fn blank_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut depth = 0usize;
    while i < bytes.len() {
        if depth == 0 && bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            depth += 1;
            out.push(b' ');
            out.push(b' ');
            i += 2;
            continue;
        }
        if depth > 0 && bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
            depth -= 1;
            out.push(b' ');
            out.push(b' ');
            i += 2;
            continue;
        }
        // Inside a comment everything becomes a space — except a newline, so line numbers
        // and the line-oriented scans below still line up.
        out.push(if depth > 0 && bytes[i] != b'\n' { b' ' } else { bytes[i] });
        i += 1;
    }
    // Every byte was either copied or replaced by an ASCII space, so this is still UTF-8 —
    // a multi-byte character inside a comment becomes that many spaces.
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The identifier starting at `at`, if one does.
fn ident_at(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    if at >= bytes.len() || !(bytes[at].is_ascii_alphabetic() || bytes[at] == b'_') {
        return None;
    }
    let mut end = at;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    Some((at, end))
}

/// The next identifier at or after `from`, skipping whitespace only. `None` if something
/// else comes first — which is how a malformed declaration is skipped instead of guessed at.
fn ident_after(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    ident_at(bytes, i)
}

/// Whether `at` starts a word — i.e. the byte before it is not part of an identifier. What
/// keeps `fn` from matching inside `my_fn`.
fn at_word_start(bytes: &[u8], at: usize) -> bool {
    at == 0 || !is_ident_byte(bytes[at - 1])
}

/// Whether the keyword at `at` is at **module scope**, tracked by brace depth. What keeps a
/// local `var` inside a function body out of the file's outline.
fn keyword_at(bytes: &[u8], at: usize, word: &[u8]) -> bool {
    at_word_start(bytes, at)
        && bytes[at..].starts_with(word)
        && bytes.get(at + word.len()).is_none_or(|b| !is_ident_byte(*b))
}

/// The attributes immediately before `at`, as a lowercase string. Read backwards over
/// whitespace and `@name(...)` runs, which is exactly what sits between an attribute and the
/// declaration it applies to.
fn attributes_before(bytes: &[u8], at: usize) -> String {
    let mut start = at;
    let mut depth = 0usize;
    while start > 0 {
        let b = bytes[start - 1];
        if b == b')' {
            depth += 1;
        } else if b == b'(' {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else if depth == 0 && !(b as char).is_whitespace() && !is_ident_byte(b) && b != b'@'
            && b != b','
        {
            break;
        }
        start -= 1;
    }
    String::from_utf8_lossy(&bytes[start..at]).to_ascii_lowercase()
}

/// Every declaration in `source`, in the order it appears.
pub fn scan(source: &str) -> Vec<WgslSymbol> {
    let blanked = blank_comments(source);
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
                continue;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
                continue;
            }
            _ => {}
        }
        // Only module scope. A `let` inside a function is a local, and an outline that
        // listed every local would be a listing of the file rather than a map of it.
        if depth > 0 {
            i += 1;
            continue;
        }

        if keyword_at(bytes, i, b"fn") {
            if let Some((s, e)) = ident_after(bytes, i + 2) {
                let attrs = attributes_before(bytes, i);
                let stage = ["@vertex", "@fragment", "@compute"]
                    .iter()
                    .find(|a| attrs.contains(*a))
                    .map(|a| a.trim_start_matches('@'));
                out.push(WgslSymbol {
                    name: blanked[s..e].to_string(),
                    kind: if stage.is_some() {
                        WgslSymbolKind::EntryPoint
                    } else {
                        WgslSymbolKind::Function
                    },
                    start: s,
                    end: e,
                    detail: stage.unwrap_or("fn").to_string(),
                    container: None,
                });
                i = e;
                continue;
            }
        }
        if keyword_at(bytes, i, b"struct") {
            if let Some((s, e)) = ident_after(bytes, i + 6) {
                let name = blanked[s..e].to_string();
                out.push(WgslSymbol {
                    name: name.clone(),
                    kind: WgslSymbolKind::Struct,
                    start: s,
                    end: e,
                    detail: "struct".into(),
                    container: None,
                });
                out.extend(fields_of(&blanked, e, &name));
                i = e;
                continue;
            }
        }
        for (word, kind, detail) in [
            (&b"alias"[..], WgslSymbolKind::Alias, "alias"),
            (&b"const"[..], WgslSymbolKind::Const, "const"),
            (&b"override"[..], WgslSymbolKind::Override, "override"),
        ] {
            if keyword_at(bytes, i, word) {
                if let Some((s, e)) = ident_after(bytes, i + word.len()) {
                    out.push(WgslSymbol {
                        name: blanked[s..e].to_string(),
                        kind,
                        start: s,
                        end: e,
                        detail: detail.into(),
                        container: None,
                    });
                    i = e;
                }
            }
        }
        if keyword_at(bytes, i, b"var") {
            // `var<uniform>` / `var<storage, read>` / plain `var`. The address space is the
            // useful half of what a global variable is, so it goes in `detail` rather than
            // being skipped over on the way to the name.
            let mut j = i + 3;
            let mut space = String::new();
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'<') {
                let close = blanked[j..].find('>').map(|k| j + k);
                if let Some(close) = close {
                    space = blanked[j + 1..close].trim().to_string();
                    j = close + 1;
                }
            }
            if let Some((s, e)) = ident_after(bytes, j) {
                let attrs = attributes_before(bytes, i);
                let binding = binding_of(&attrs);
                out.push(WgslSymbol {
                    name: blanked[s..e].to_string(),
                    kind: WgslSymbolKind::Var,
                    start: s,
                    end: e,
                    detail: match (space.is_empty(), binding) {
                        (true, None) => "var".to_string(),
                        (false, None) => format!("var<{space}>"),
                        (true, Some(b)) => b,
                        (false, Some(b)) => format!("var<{space}> {b}"),
                    },
                    container: None,
                });
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `@group(1) @binding(0)` → `"@group(1) @binding(0)"`, or `None` when there is no binding.
/// Shown beside a global because "which slot is this" is the question a shader's bindings
/// are read to answer.
fn binding_of(attrs: &str) -> Option<String> {
    let group = between(attrs, "@group(")?;
    let binding = between(attrs, "@binding(")?;
    Some(format!("@group({group}) @binding({binding})"))
}

fn between(text: &str, open: &str) -> Option<String> {
    let at = text.find(open)? + open.len();
    let close = text[at..].find(')')? + at;
    Some(text[at..close].trim().to_string())
}

/// The members declared in the struct body that starts after `from`.
fn fields_of(src: &str, from: usize, owner: &str) -> Vec<WgslSymbol> {
    let bytes = src.as_bytes();
    let Some(open) = src[from..].find('{').map(|k| from + k) else { return Vec::new() };
    let mut out = Vec::new();
    let mut i = open + 1;
    let mut depth = 1usize;
    // A member is `name : type`, possibly behind attributes. Read at brace depth 1 only, so
    // a nested generic's contents cannot be mistaken for members.
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {
                if depth == 1 {
                    if let Some((s, e)) = ident_at(bytes, i) {
                        let mut k = e;
                        while k < bytes.len() && (bytes[k] as char).is_whitespace() {
                            k += 1;
                        }
                        if bytes.get(k) == Some(&b':') {
                            out.push(WgslSymbol {
                                name: src[s..e].to_string(),
                                kind: WgslSymbolKind::Field,
                                start: s,
                                end: e,
                                detail: "field".into(),
                                container: Some(owner.to_string()),
                            });
                        }
                        i = e;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// The identifier under `offset`, as `(name, start, end)`.
///
/// Offsets inside a comment answer `None`: the word is there, but it is not a symbol, and
/// find-usages on a word in a comment is a result list nobody asked for.
pub fn symbol_at(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    let blanked = blank_comments(source);
    let bytes = blanked.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    // Walk back to the start of the word the caret is inside or just after.
    let mut start = offset.min(bytes.len());
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let (s, e) = ident_at(bytes, start)?;
    if offset > e {
        return None;
    }
    Some((blanked[s..e].to_string(), s, e))
}

/// Every occurrence of the identifier `name`, as byte ranges.
///
/// Whole-word and comment-free, which is the whole difference between this and a text
/// search: `view` does not match `view_proj`, and a mention in a `//` line is not a use.
/// Scoped to the file, because WGSL has no imports — anything wider is naga_oil's
/// composition graph, and answering "who uses this" across it would mean claiming to know a
/// resolution this crate deliberately does not do.
pub fn occurrences_of(source: &str, name: &str) -> Vec<(usize, usize)> {
    if name.is_empty() {
        return Vec::new();
    }
    let blanked = blank_comments(source);
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(hit) = blanked[from..].find(name) {
        let s = from + hit;
        let e = s + name.len();
        if at_word_start(bytes, s) && bytes.get(e).is_none_or(|b| !is_ident_byte(*b)) {
            out.push((s, e));
        }
        from = e.max(s + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = r#"
// a comment mentioning view
struct View {
    clip_from_world: mat4x4<f32>,
    world_position: vec3<f32>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var base_color_texture: texture_2d<f32>;

const PI: f32 = 3.14159;
alias Colour = vec4<f32>;

fn helper(x: f32) -> f32 {
    var local = x;      // must NOT be in the outline
    return local * PI;
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(view.world_position, helper(1.0));
}
"#;

    fn named<'a>(all: &'a [WgslSymbol], name: &str) -> &'a WgslSymbol {
        all.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name} not found"))
    }

    #[test]
    fn finds_every_module_scope_declaration() {
        let all = scan(SHADER);
        for n in ["View", "view", "base_color_texture", "PI", "Colour", "helper", "fragment"] {
            let _ = named(&all, n);
        }
    }

    #[test]
    fn a_local_var_is_not_a_declaration_of_the_file() {
        let all = scan(SHADER);
        assert!(all.iter().all(|s| s.name != "local"), "locals belong to the function, not the outline");
    }

    #[test]
    fn an_entry_point_is_told_apart_from_a_function() {
        let all = scan(SHADER);
        assert_eq!(named(&all, "fragment").kind, WgslSymbolKind::EntryPoint);
        assert_eq!(named(&all, "fragment").detail, "fragment");
        assert_eq!(named(&all, "helper").kind, WgslSymbolKind::Function);
    }

    #[test]
    fn a_global_carries_its_binding() {
        let all = scan(SHADER);
        assert_eq!(named(&all, "view").detail, "var<uniform> @group(0) @binding(0)");
        assert_eq!(named(&all, "base_color_texture").detail, "@group(0) @binding(1)");
    }

    #[test]
    fn struct_members_are_found_and_know_their_struct() {
        let all = scan(SHADER);
        let f = named(&all, "world_position");
        assert_eq!(f.kind, WgslSymbolKind::Field);
        assert_eq!(f.container.as_deref(), Some("View"));
    }

    #[test]
    fn the_name_span_points_at_the_name() {
        let all = scan(SHADER);
        let s = named(&all, "helper");
        assert_eq!(&SHADER[s.start..s.end], "helper");
    }

    #[test]
    fn occurrences_are_whole_words_and_skip_comments() {
        let hits = occurrences_of(SHADER, "view");
        // The declaration and the use in `view.world_position` — not the comment, and not
        // the `View` type (different case), and not `world_position`.
        assert_eq!(hits.len(), 2, "got {hits:?}");
        assert!(hits.iter().all(|(s, e)| &SHADER[*s..*e] == "view"));
    }

    #[test]
    fn a_nested_block_comment_does_not_swallow_the_file() {
        let src = "/* outer /* inner */ still a comment */ fn after() {}";
        let all = scan(src);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "after");
    }

    #[test]
    fn the_symbol_under_the_caret_is_the_whole_word() {
        let at = SHADER.find("helper(1.0)").unwrap() + 2;
        assert_eq!(symbol_at(SHADER, at).map(|(n, _, _)| n), Some("helper".to_string()));
    }

    #[test]
    fn a_word_in_a_comment_is_not_a_symbol() {
        let at = SHADER.find("a comment mentioning view").unwrap() + 2;
        assert_eq!(symbol_at(SHADER, at), None);
    }
}

/// The `//` comment block immediately above `start`, as documentation.
///
/// The convention a shader is actually written with — WGSL has no doc-comment syntax, so
/// the lines directly above a declaration are the documentation, and treating them as such
/// is the difference between a hover that says something and one that repeats the signature.
///
/// Stops at the first blank line or non-comment line, so the paragraph belonging to the
/// declaration above it does not leak onto this one.
pub fn doc_above(source: &str, start: usize) -> Option<String> {
    let head = &source[..start.min(source.len())];
    // The declaration's own line first: `start` points at the name, not at the line start.
    let mut lines: Vec<&str> = head.lines().collect();
    lines.pop();

    let mut collected: Vec<String> = Vec::new();
    for line in lines.iter().rev() {
        let t = line.trim();
        // Attributes sit between the comment and the declaration (`@fragment` above a `fn`),
        // and skipping them is what lets an entry point keep its documentation.
        if t.starts_with('@') || t.is_empty() && collected.is_empty() {
            if t.is_empty() {
                break;
            }
            continue;
        }
        let Some(rest) = t.strip_prefix("//") else { break };
        collected.push(rest.trim_start_matches('/').trim().to_string());
    }
    if collected.is_empty() {
        return None;
    }
    collected.reverse();
    Some(collected.join("\n"))
}

/// The declaration's own line, trimmed — what a hover shows as the signature.
///
/// The line rather than a reconstruction: a WGSL signature is already one line in almost
/// every case, and printing the source is both correct and recognisably the thing the user
/// is looking at. A trailing `{` is dropped because a signature is not a body.
pub fn signature_at(source: &str, start: usize) -> String {
    let from = source[..start.min(source.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = source[from..].lines().next().unwrap_or("");
    line.trim().trim_end_matches('{').trim().trim_end_matches(',').to_string()
}

#[cfg(test)]
mod doc_tests {
    use super::*;

    const SRC: &str = "// Two-dimensional hash.\n\
                       // Not cryptographic.\n\
                       @fragment\n\
                       fn hash(p: vec2<f32>) -> f32 {\n\
                       }\n";

    #[test]
    fn the_comment_block_above_a_declaration_is_its_documentation() {
        let at = SRC.find("hash(p").unwrap();
        assert_eq!(
            doc_above(SRC, at).as_deref(),
            Some("Two-dimensional hash.\nNot cryptographic."),
            "attributes between the comment and the declaration must not break the block"
        );
    }

    #[test]
    fn a_blank_line_ends_the_block() {
        let src = "// belongs to something else\n\nfn f() {}\n";
        assert_eq!(doc_above(src, src.find("f()").unwrap()), None);
    }

    #[test]
    fn the_signature_is_the_line_without_its_body() {
        let at = SRC.find("hash(p").unwrap();
        assert_eq!(signature_at(SRC, at), "fn hash(p: vec2<f32>) -> f32");
    }
}
