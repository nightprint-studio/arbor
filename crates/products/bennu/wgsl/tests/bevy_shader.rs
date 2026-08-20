//! The scanner against a shader shaped like the ones it is actually pointed at.
//!
//! The unit tests in `symbols.rs` feed it fragments that isolate one rule each, which is
//! what unit tests are for and also what let a whole class of problem through: a real Bevy
//! shader is a *composition* of the things they test separately, and the interesting
//! failures live in the seams — a braced `#import` spanning four lines, a `#{SHADER_DEF}`
//! sitting inside an attribute, an entry point whose name is also the name of its stage.
//!
//! So this file asks the questions the editor asks, in the order it asks them, against one
//! fixture that has all of it at once. Go-to and find-usages are the two that go quiet
//! first when the scanner is confused, so they are the two asserted hardest.

use bennu_wgsl::prelude::*;

const SHADER: &str = include_str!("fixtures/bevy_material.wgsl");

/// Byte offset of a caret sitting **inside** the `nth` occurrence of `needle`.
///
/// Inside rather than at its start, because that is where a caret actually is when somebody
/// Ctrl+clicks a name, and "the word under the caret" is exactly the step that has to cope
/// with it.
fn caret(needle: &str, nth: usize) -> usize {
    let mut from = 0;
    for _ in 0..nth {
        from = SHADER[from..].find(needle).expect("not enough occurrences") + from + needle.len();
    }
    SHADER[from..].find(needle).expect("not enough occurrences") + from + needle.len() / 2
}

#[test]
fn every_declaration_in_the_file_is_found() {
    let symbols = scan_symbols(SHADER);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    for expected in [
        "SpiralParams",
        "params",
        "MASK_HALF",
        "sd_round_box",
        "hash",
        "value_noise",
        "fbm",
        "fragment",
    ] {
        assert!(names.contains(&expected), "`{expected}` missing from {names:?}");
    }
}

#[test]
fn a_shader_def_inside_an_attribute_does_not_hide_the_var_under_it() {
    // `@group(#{MATERIAL_BIND_GROUP}) @binding(0)` is not WGSL — naga_oil substitutes it
    // before the compiler ever sees it. A scanner that treats `#` as end-of-story loses
    // every uniform in every Bevy material, which is the single most looked-up name in one.
    let params = scan_symbols(SHADER)
        .into_iter()
        .find(|s| s.name == "params")
        .expect("the uniform was not scanned");
    assert_eq!(params.kind, WgslSymbolKind::Var);
    assert_eq!(&SHADER[params.start..params.end], "params");
}

#[test]
fn the_entry_point_is_marked_as_one() {
    let f = scan_symbols(SHADER).into_iter().find(|s| s.name == "fragment").unwrap();
    assert_eq!(f.kind, WgslSymbolKind::EntryPoint);
}

#[test]
fn go_to_declaration_from_a_call_lands_on_the_function() {
    // The caret is on the CALL of `value_noise` inside `fbm`; the answer must be the `fn`.
    let at = caret("value_noise(freq)", 0);
    let (name, _, _) = symbol_at(SHADER, at).expect("the caret is on a word");
    assert_eq!(name, "value_noise");

    let decl = scan_symbols(SHADER).into_iter().find(|s| s.name == name).unwrap();
    assert_eq!(decl.kind, WgslSymbolKind::Function);
    // The declaration is the `fn value_noise`, not the call site the caret is in.
    assert!(decl.start < at);
    assert_eq!(&SHADER[decl.start..decl.end], "value_noise");
    assert!(SHADER[..decl.start].trim_end().ends_with("fn"));
}

#[test]
fn go_to_declaration_reaches_a_function_declared_further_up() {
    let at = caret("sd_round_box(p, vec2", 0);
    let (name, _, _) = symbol_at(SHADER, at).unwrap();
    let decl = scan_symbols(SHADER).into_iter().find(|s| s.name == name).unwrap();
    assert!(SHADER[..decl.start].trim_end().ends_with("fn"));
}

#[test]
fn find_usages_on_the_uniform_reports_the_declaration_and_every_read() {
    let hits = occurrences_of(SHADER, "params");
    // Declared once, then read three times in `fragment`.
    assert_eq!(hits.len(), 4, "got {hits:?}");
    for (start, end) in &hits {
        assert_eq!(&SHADER[*start..*end], "params");
    }
}

#[test]
fn find_usages_does_not_match_a_name_inside_a_longer_one() {
    // `hash` is a function; `p3` and the word `hash` inside no other identifier here — but
    // the guard that makes that true is worth pinning, because the day it breaks the
    // symptom is a usages list with plausible-looking noise in it rather than an error.
    let hits = occurrences_of(SHADER, "hash");
    assert_eq!(hits.len(), 3, "got {hits:?}");
}

#[test]
fn find_usages_ignores_the_word_when_it_is_only_in_a_comment() {
    // "Hash without Sine" in the comment above `fn hash` must not be a usage: comments are
    // blanked before the scan, and a usages panel that lists prose is a usages panel nobody
    // trusts the second time.
    let hits = occurrences_of(SHADER, "Hash");
    assert!(hits.is_empty(), "a comment word was reported as a usage: {hits:?}");
}

#[test]
fn hover_on_a_declaration_finds_the_comment_above_it() {
    let decl = scan_symbols(SHADER).into_iter().find(|s| s.name == "sd_round_box").unwrap();
    let doc = doc_above(SHADER, decl.start).expect("the comment above it is its documentation");
    assert!(doc.contains("Inigo Quilez"), "got {doc:?}");
    let sig = signature_at(SHADER, decl.start);
    assert!(sig.contains("fn sd_round_box"), "got {sig:?}");
}

#[test]
fn the_braced_import_is_understood_as_a_module_list() {
    // The caret just after `forward_io::` on the second line of the braced form: the
    // `#import` keyword is three lines up, so a scan that only looks at the current line
    // sees an ordinary identifier and offers the language's built-ins inside an import.
    let at = SHADER.find("forward_io::VertexOutput").unwrap() + "forward_io::".len();
    assert!(
        matches!(import_context_at(SHADER, at), Some(_)),
        "the caret inside a braced #import was not recognised as one"
    );
}

#[test]
fn a_composed_shader_is_not_compiled() {
    // naga cannot parse `#import`. Reporting its parse error would put a red squiggle on
    // line 8 of every Bevy shader in the project, which is worse than saying nothing.
    let report = validate(SHADER);
    assert!(report.skipped.is_some(), "a naga_oil shader was sent to the compiler");
    assert!(report.diagnostics.is_empty());
}
