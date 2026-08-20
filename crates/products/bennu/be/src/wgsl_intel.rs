//! `wgsl_intel` — WGSL intelligence when no language server is running.
//!
//! Every function here is a **fallback**: `bennu-be` asks the language-server route first,
//! and `wgsl-analyzer` (registered in the LSP catalogue) answers instead of any of this
//! when it is installed. What lives here is what a shader gets with nothing installed —
//! which, for a `.wgsl` sitting in a Bevy project, is the ordinary case.
//!
//! Two engines behind it, and the split is deliberate (see `bennu-wgsl`): **naga** for
//! diagnostics, because it is the compiler wgpu really runs and its verdict is the one that
//! matters; a tolerant **scanner** for completion, structure and find-usages, because those
//! are wanted while the file is still half-written and a compiler has nothing to say then.
//!
//! Each entry point returns `Option`, `None` meaning "not mine" — the same shape
//! `cargo_intel` uses, so `intel.rs` reads as a list of routes rather than as a tree of ifs.

use std::path::{Path, PathBuf};

use bennu_proto::prelude::{
    CompletionItem, DeclarationTarget, Diagnostic, HoverInfo, UsageHit, UsagesResult,
};
use bennu_wgsl::prelude::{
    completions_for, defined_path, doc_above, import_context_at, occurrences_of, scan_symbols,
    signature_at, symbol_at, validate, ImportContext, WgslSeverity, WgslSymbolKind,
    ATTRIBUTES, BEVY_IMPORTS, BUILTIN_FUNCTIONS, BUILTIN_TYPES, BUILTIN_VALUES, KEYWORDS,
};

/// Whether this file is ours.
pub(crate) fn is_wgsl(file: &str) -> bool {
    file.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("wgsl"))
}

/// Read the buffer the caller sent, or the file from disk. A shader is small, and answering
/// about the saved file is far better than answering nothing when the caller had no buffer.
fn text_of(file: &str, source: Option<&str>) -> Option<String> {
    match source {
        Some(s) => Some(s.to_string()),
        None => std::fs::read_to_string(file).ok(),
    }
}

/// Compiler diagnostics for a shader. `None` for a file that is not WGSL.
///
/// A file composed with naga_oil comes back **empty** rather than red: half its identifiers
/// are declared in the module it imports, so compiling it alone would report a hundred
/// problems on a shader that is correct. See `bennu_wgsl::validate::preprocessor_reason`.
pub(crate) fn diagnostics(file: &str, source: Option<&str>) -> Option<Vec<Diagnostic>> {
    if !is_wgsl(file) {
        return None;
    }
    let text = text_of(file, source)?;
    let report = validate(&text);
    Some(
        report
            .diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                start: d.start,
                end: d.end,
                severity: match d.severity {
                    WgslSeverity::Error => "error".to_string(),
                    WgslSeverity::Warning => "warning".to_string(),
                },
                // One kind, and it is the honest one: this is what the shader compiler
                // said. Splitting it into a taxonomy would mean inventing categories naga
                // does not have.
                code: "wgsl-compile".to_string(),
                message: d.message,
            })
            .collect(),
    )
}

/// The identifier prefix immediately before `offset`, and where it starts.
fn prefix_at(text: &str, offset: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut start = offset.min(bytes.len());
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    (text[start..offset.min(text.len())].to_string(), start)
}

/// Completion for a shader: what the file declares, then what the language offers.
///
/// The file's own names first, always. In a shader the thing you are about to type is
/// overwhelmingly a binding or a struct field you declared thirty lines up — and burying
/// those under a hundred built-ins is how a completion list stops being used.
pub(crate) fn completion(
    file: &str,
    offset: usize,
    source: Option<&str>,
) -> Option<Vec<CompletionItem>> {
    if !is_wgsl(file) {
        return None;
    }
    let text = text_of(file, source)?;
    // An `#import` line first: what belongs there is module paths, and the file's own
    // functions would be noise in the one place they cannot be written.
    if let Some(items) = import_completion(file, &text, offset) {
        return Some(items);
    }
    let (prefix, start) = prefix_at(&text, offset);
    let lower = prefix.to_ascii_lowercase();

    let item = |label: String, kind: &str, detail: String| CompletionItem {
        label,
        kind: kind.to_string(),
        detail: Some(detail),
        auto_import: None,
        insert_text: None,
        // The provider knows exactly what the accept replaces, so it says so rather than
        // leaving the frontend to re-derive the word boundary from the buffer.
        replace_start: Some(start),
        replace_end: Some(offset),
        ..Default::default()
    };

    let mut out: Vec<CompletionItem> = scan_symbols(&text)
        .into_iter()
        .filter(|s| lower.is_empty() || s.name.to_ascii_lowercase().starts_with(&lower))
        .map(|s| {
            let kind = match s.kind {
                WgslSymbolKind::Function | WgslSymbolKind::EntryPoint => "function",
                WgslSymbolKind::Struct => "class",
                WgslSymbolKind::Field => "field",
                WgslSymbolKind::Alias => "class",
                WgslSymbolKind::Const | WgslSymbolKind::Override => "constant",
                WgslSymbolKind::Var => "variable",
            };
            let detail = match &s.container {
                Some(owner) => format!("{owner}.{} — {}", s.name, s.detail),
                None => s.detail.clone(),
            };
            item(s.name, kind, detail)
        })
        .collect();

    out.extend(completions_for(&prefix).into_iter().map(|b| {
        let kind = if b.name.starts_with('@') {
            "keyword"
        } else if b.detail.contains("space") || b.detail.contains("@builtin") {
            "constant"
        } else if b.name.chars().next().is_some_and(|c| c.is_lowercase()) && b.detail.len() > 2 {
            "function"
        } else {
            "keyword"
        };
        item(b.name.to_string(), kind, b.detail.to_string())
    }));
    Some(out)
}

/// Find-usages for a shader, scoped to the file.
///
/// The file and nothing wider, on purpose: WGSL has no imports, so anything beyond it is
/// naga_oil's composition graph — and claiming to resolve that would mean claiming a
/// resolution this side deliberately does not do. Better a true answer about one file than
/// a plausible one about several.
pub(crate) fn references(
    file: &str,
    source: &str,
    offset: usize,
) -> Option<Option<UsagesResult>> {
    if !is_wgsl(file) {
        return None;
    }
    let Some((name, _, _)) = symbol_at(source, offset) else { return Some(None) };
    let declared = scan_symbols(source).into_iter().find(|s| s.name == name);
    let hits: Vec<UsageHit> = occurrences_of(source, &name)
        .into_iter()
        .map(|(start, end)| {
            let (line, col) = line_col(source, start);
            UsageHit {
                file: file.to_string(),
                start,
                end,
                line,
                col,
                preview: source[line_start(source, start)..]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            }
        })
        .collect();
    if hits.is_empty() {
        return Some(None);
    }
    let label = match declared {
        Some(s) => format!("{} {}", s.detail, s.name),
        // Not declared in this file: a name from an imported module, or a built-in. Said
        // plainly, because "0 declarations, 4 uses" is a different situation from a local
        // symbol and the label is where the user finds that out.
        None => format!("{name} (not declared in this file)"),
    };
    Some(Some(UsagesResult { target_label: label, usages: hits }))
}

/// Byte offset of the start of the line containing `at`.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// 1-based line and column (in characters) of `at`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let line = text[..at].matches('\n').count() + 1;
    let col = text[line_start(text, at)..at].chars().count() + 1;
    (line, col)
}

// ── go to declaration ───────────────────────────────────────────────────────────

/// Where the name under the caret is declared, within this file.
///
/// Within the file, like find-usages and for the same reason: anything wider is naga_oil's
/// composition graph, and a jump into a file this side has not resolved would be a guess
/// wearing the clothes of an answer.
pub(crate) fn declaration(
    file: &str,
    source: &str,
    offset: usize,
) -> Option<Option<DeclarationTarget>> {
    if !is_wgsl(file) {
        return None;
    }
    let Some((name, _, _)) = symbol_at(source, offset) else { return Some(None) };
    let Some(sym) = scan_symbols(source).into_iter().find(|s| s.name == name) else {
        return Some(None);
    };
    let (line, col) = line_col(source, sym.start);
    Some(Some(DeclarationTarget {
        file: file.to_string(),
        start: sym.start,
        end: sym.end,
        line: line as u32,
        col: col as u32,
        label: format!("{} {}", sym.detail, sym.name),
    }))
}

// ── hover ───────────────────────────────────────────────────────────────────────

/// What the name under the caret is, and whatever documentation it has.
///
/// Three answers in order of how much they know. A **declaration in this file** gets its own
/// source line as the signature and the comment block above it as documentation — WGSL has
/// no doc-comment syntax, so those lines *are* the documentation and treating them as
/// anything else throws away the only thing the author wrote down. A **built-in** gets what
/// the language says it is. Anything else gets nothing, rather than a card that says the
/// name back at you.
pub(crate) fn hover(file: &str, source: &str, offset: usize) -> Option<Option<HoverInfo>> {
    if !is_wgsl(file) {
        return None;
    }
    let Some((name, start, _)) = symbol_at(source, offset) else { return Some(None) };

    // The `@` first, and before anything else. An entry point is conventionally named after
    // the attribute that marks it — `@fragment` sits directly above `fn fragment` in almost
    // every Bevy shader — so a declaration lookup that runs first answers `@fragment` with
    // the function, every time. The `@` is what tells the two apart, and it is not part of
    // the word the caret landed on.
    if start > 0 && source.as_bytes()[start - 1] == b'@' {
        let attributed = format!("@{name}");
        return Some(ATTRIBUTES.iter().find(|b| b.name == attributed).map(|b| HoverInfo {
            signature: b.name.to_string(),
            kind: "method".to_string(),
            container: Some("WGSL attribute".to_string()),
            doc: Some(b.detail.to_string()),
        }));
    }

    if let Some(sym) = scan_symbols(source).into_iter().find(|s| s.name == name) {
        return Some(Some(HoverInfo {
            signature: signature_at(source, sym.start),
            kind: match sym.kind {
                WgslSymbolKind::Function | WgslSymbolKind::EntryPoint => "method",
                WgslSymbolKind::Struct | WgslSymbolKind::Alias => "class",
                WgslSymbolKind::Field => "field",
                _ => "field",
            }
            .to_string(),
            container: sym.container.clone(),
            doc: doc_above(source, sym.start),
        }));
    }

    let builtin = BUILTIN_FUNCTIONS
        .iter()
        .find(|b| b.name == name)
        .or_else(|| BUILTIN_TYPES.iter().find(|b| b.name == name))
        .or_else(|| BUILTIN_VALUES.iter().find(|b| b.name == name))
        .or_else(|| KEYWORDS.iter().find(|b| b.name == name));

    let Some(b) = builtin else { return Some(None) };
    Some(Some(HoverInfo {
        signature: b.name.to_string(),
        kind: "method".to_string(),
        container: Some("WGSL".to_string()),
        doc: Some(b.detail.to_string()),
    }))
}

// ── import completion ───────────────────────────────────────────────────────────

/// How far the walk for `#define_import_path` will go before giving up.
///
/// A completion runs while the user types, and an unbounded walk of somebody's monorepo is
/// not something to do on a keystroke. Only reached on an import line — a handful of
/// keystrokes per file — so the bound is generous rather than tight.
const MAX_SCANNED: usize = 1_500;

/// The nearest ancestor that looks like a project root.
fn project_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .find(|p| p.join("Cargo.toml").is_file() || p.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Every `#define_import_path` the project declares, with the file that declares it.
///
/// The project's own modules — the half of the answer no catalogue can hold, because the
/// user wrote them. Bevy's own live inside the `bevy_*` crates' sources, wherever cargo put
/// them, and guessing at that from an editor is guessing about somebody's machine; those
/// come from the curated list instead.
fn project_modules(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        if seen >= MAX_SCANNED {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                // The three that are always large and never hold a shader anybody imports.
                if !matches!(name.as_str(), "target" | ".git" | "node_modules") {
                    stack.push(p);
                }
                continue;
            }
            if !name.ends_with(".wgsl") {
                continue;
            }
            seen += 1;
            if let Some(path) = std::fs::read_to_string(&p).ok().and_then(|s| defined_path(&s)) {
                out.push((path, p));
            }
        }
    }
    out
}

/// Completion for an `#import` line, or `None` when the caret is not on one.
fn import_completion(file: &str, source: &str, offset: usize) -> Option<Vec<CompletionItem>> {
    let ctx = import_context_at(source, offset)?;
    let (prefix, start, package) = match &ctx {
        ImportContext::Module { prefix, start } => (prefix.clone(), *start, None),
        ImportContext::Item { prefix, start, package } => {
            (prefix.clone(), *start, Some(package.clone()))
        }
    };
    let lower = prefix.to_ascii_lowercase();
    let item = |label: String, kind: &str, detail: String| CompletionItem {
        label,
        kind: kind.to_string(),
        detail: Some(detail),
        replace_start: Some(start),
        replace_end: Some(offset),
        ..Default::default()
    };

    let modules = project_root(Path::new(file)).map(|r| project_modules(&r)).unwrap_or_default();
    let mut out = Vec::new();

    // The project's own first: a shader in this repo is the thing you are most likely to be
    // importing, and it is the half a catalogue can never know.
    for (path, at) in &modules {
        let offer = match &package {
            // Inside `pkg::{ … }` only the tail is typed, so only the tail is offered.
            Some(pkg) => path.strip_prefix(&format!("{pkg}::")).map(str::to_string),
            None => Some(path.clone()),
        };
        let Some(label) = offer else { continue };
        if lower.is_empty() || label.to_ascii_lowercase().starts_with(&lower) {
            let where_ = at.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(item(label, "module", format!("this project — {where_}")));
        }
    }

    for b in BEVY_IMPORTS {
        let offer = match &package {
            Some(pkg) => b.path.strip_prefix(&format!("{pkg}::")).map(str::to_string),
            None => Some(b.path.to_string()),
        };
        let Some(label) = offer else { continue };
        if lower.is_empty() || label.to_ascii_lowercase().starts_with(&lower) {
            out.push(item(label, "module", b.detail.to_string()));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = "\
// Signed distance to a rounded box.
// From Inigo Quilez.
fn sd_round_box(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(sd_round_box(vec2<f32>(0.0), 0.1));
}
";

    #[test]
    fn go_to_declaration_lands_on_the_name() {
        // The caret on the CALL, which is the direction a go-to is actually used in.
        let at = SHADER.rfind("sd_round_box").unwrap() + 3;
        let target = declaration("s.wgsl", SHADER, at).unwrap().unwrap();
        assert_eq!(&SHADER[target.start..target.end], "sd_round_box");
        assert_eq!(target.line, 3, "the declaration is on line 3");
        assert!(target.label.contains("sd_round_box"));
    }

    #[test]
    fn hover_on_your_own_function_shows_its_signature_and_its_comment() {
        let at = SHADER.rfind("sd_round_box").unwrap() + 3;
        let card = hover("s.wgsl", SHADER, at).unwrap().unwrap();
        assert_eq!(card.signature, "fn sd_round_box(p: vec2<f32>, r: f32) -> f32");
        assert_eq!(
            card.doc.as_deref(),
            Some("Signed distance to a rounded box.\nFrom Inigo Quilez."),
            "WGSL has no doc-comment syntax, so the block above IS the documentation"
        );
    }

    #[test]
    fn hover_on_a_builtin_says_what_the_language_says() {
        let at = SHADER.find("length(p)").unwrap() + 2;
        let card = hover("s.wgsl", SHADER, at).unwrap().unwrap();
        assert_eq!(card.container.as_deref(), Some("WGSL"));
        assert_eq!(card.doc.as_deref(), Some("vector length"));
    }

    #[test]
    fn an_unknown_name_gets_no_card_rather_than_one_repeating_it() {
        let src = "fn f() { let x = qqqq; }";
        assert_eq!(hover("s.wgsl", src, src.find("qqqq").unwrap() + 1), Some(None));
    }

    #[test]
    fn an_import_line_completes_modules_and_nothing_else() {
        let src = "#import bevy_pbr::mesh_view";
        let items = completion("s.wgsl", src.len(), Some(src)).unwrap();
        assert!(
            items.iter().any(|i| i.label == "bevy_pbr::mesh_view_bindings"),
            "got {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
        assert!(
            items.iter().all(|i| i.kind == "module"),
            "an import line has no room for a function"
        );
    }

    #[test]
    fn a_braced_import_completes_only_the_tail() {
        let src = "#import bevy_pbr::{\n    forward_io::VertexOutput,\n    mesh_view_bind";
        let items = completion("s.wgsl", src.len(), Some(src)).unwrap();
        assert!(items.iter().any(|i| i.label == "mesh_view_bindings"), "the package is implied");
        assert!(
            !items.iter().any(|i| i.label.starts_with("bevy_pbr::")),
            "offering the full path inside the braces would insert it twice"
        );
    }

    const SRC: &str = "@group(0) @binding(0) var<uniform> view: mat4x4<f32>;\n\
                       fn use_it() -> mat4x4<f32> { return view; }\n";

    #[test]
    fn only_wgsl_is_claimed() {
        assert!(diagnostics("a.rs", Some("fn main() {}")).is_none());
        assert!(diagnostics("a.WGSL", Some("")).is_some());
    }

    #[test]
    fn the_files_own_names_come_before_the_language() {
        let at = SRC.find("return view").unwrap() + "return vi".len();
        let items = completion("s.wgsl", at, Some(SRC)).unwrap();
        assert_eq!(items.first().map(|i| i.label.as_str()), Some("view"));
    }

    #[test]
    fn completion_says_what_it_replaces() {
        let at = SRC.find("return view").unwrap() + "return vi".len();
        let items = completion("s.wgsl", at, Some(SRC)).unwrap();
        let first = &items[0];
        assert_eq!(first.replace_end, Some(at));
        assert_eq!(&SRC[first.replace_start.unwrap()..at], "vi");
    }

    #[test]
    fn usages_find_the_declaration_and_the_use() {
        let at = SRC.find("return view").unwrap() + "return v".len();
        let result = references("s.wgsl", SRC, at).unwrap().unwrap();
        assert_eq!(result.usages.len(), 2);
        assert!(result.target_label.contains("view"));
        assert_eq!(result.usages[1].line, 2);
    }

    #[test]
    fn a_composed_shader_is_not_reported_as_broken() {
        let src = "#import bevy_pbr::forward_io::VertexOutput\nfn f(v: VertexOutput) {}\n";
        assert_eq!(diagnostics("s.wgsl", Some(src)), Some(Vec::new()));
    }

    /// A Bevy material shader — braced `#import`, a `#{SHADER_DEF}` inside an attribute, an
    /// entry point named after its stage. The toy `SRC` above has none of that, and none of
    /// that is exotic: it is what every shader in a Bevy project looks like.
    const BEVY: &str = concat!(
        "#import bevy_pbr::{\n",
        "    forward_io::VertexOutput,\n",
        "    mesh_view_bindings::globals,\n",
        "}\n\n",
        "struct SpiralParams {\n",
        "    spiral_arms: f32,\n",
        "};\n\n",
        "@group(#{MATERIAL_BIND_GROUP}) @binding(0)\n",
        "var<uniform> params: SpiralParams;\n\n",
        "// Signed distance to a rounded box (Inigo Quilez).\n",
        "fn sd_round_box(p: vec2<f32>, r: f32) -> f32 {\n",
        "    return length(p) - r;\n",
        "}\n\n",
        "@fragment\n",
        "fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {\n",
        "    let d = sd_round_box(mesh.uv, params.spiral_arms);\n",
        "    return vec4<f32>(d, d, d, 1.0);\n",
        "}\n",
    );

    /// A caret in the middle of the `nth` occurrence of `needle` — where one actually is
    /// when somebody Ctrl+clicks a name.
    fn caret(needle: &str, nth: usize) -> usize {
        let mut from = 0;
        for _ in 0..nth {
            from = BEVY[from..].find(needle).expect("not enough occurrences") + from + needle.len();
        }
        BEVY[from..].find(needle).expect("not enough occurrences") + from + needle.len() / 2
    }

    #[test]
    fn go_to_declaration_from_a_call_in_a_bevy_shader() {
        let at = caret("sd_round_box", 1); // the call inside `fragment`
        let target = declaration("s.wgsl", BEVY, at).unwrap().expect("no declaration");
        assert_eq!(&BEVY[target.start..target.end], "sd_round_box");
        // The declaration, not the call the caret is in.
        assert!(target.start < at);
        assert!(target.label.contains("sd_round_box"));
    }

    #[test]
    fn find_usages_reaches_the_uniform_under_a_shader_def() {
        let at = caret("params", 1); // the read inside `fragment`
        let usages = references("s.wgsl", BEVY, at).unwrap().expect("no usages");
        assert_eq!(usages.usages.len(), 2, "{:?}", usages.usages);
        assert!(usages.target_label.contains("params"), "got {:?}", usages.target_label);
        // The declaration is one of them — IntelliJ lists it, and so does the Java engine.
        assert!(usages.usages.iter().any(|u| u.start < at));
    }

    #[test]
    fn hover_on_a_function_shows_its_signature_and_the_comment_above_it() {
        let at = caret("sd_round_box", 1);
        let card = hover("s.wgsl", BEVY, at).unwrap().expect("no hover card");
        assert!(card.signature.contains("fn sd_round_box"), "got {:?}", card.signature);
        assert!(
            card.doc.as_deref().unwrap_or("").contains("Inigo Quilez"),
            "the comment above a declaration IS its documentation in WGSL: {:?}",
            card.doc
        );
    }

    #[test]
    fn hover_on_an_entry_point_attribute_is_about_the_attribute() {
        // Two ways to get this wrong, and both are the default. The caret lands on the word,
        // not on the `@`, so a lookup that forgets to put it back misses every attribute
        // there is — and `@fragment` sits directly above `fn fragment`, so one that scans
        // declarations first answers with the function, whose name is identical.
        let at = BEVY.find("@fragment").unwrap() + "@frag".len();
        let card = hover("s.wgsl", BEVY, at).unwrap().expect("no hover card");
        assert_eq!(card.signature, "@fragment");
        assert_eq!(card.container.as_deref(), Some("WGSL attribute"));
    }

    #[test]
    fn a_name_the_file_does_not_declare_says_so_rather_than_going_quiet() {
        // `VertexOutput` comes in through the `#import`; this side does not resolve naga_oil,
        // so there is no declaration to jump to. Find-usages still answers, and the label is
        // where the user learns why there is no declaration in the list.
        let at = caret("VertexOutput", 1);
        assert_eq!(declaration("s.wgsl", BEVY, at), Some(None));
        let usages = references("s.wgsl", BEVY, at).unwrap().expect("no usages");
        assert!(usages.target_label.contains("not declared in this file"));
    }
}
