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

use bennu_proto::prelude::{
    CompletionItem, DeclarationTarget, Diagnostic, HoverInfo, UsageHit, UsagesResult,
};
use bennu_wgsl::prelude::{
    completions_for, doc_above, import_context_at, occurrences_of, scan_symbols, signature_at,
    symbol_at, validate, ImportContext, LibrarySymbol, WgslSeverity, WgslSymbolKind, ATTRIBUTES,
    BEVY_IMPORTS, BUILTIN_FUNCTIONS, BUILTIN_TYPES, BUILTIN_VALUES, KEYWORDS,
};

use crate::wgsl_library;

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
    // An `#import` line first: what belongs there is module paths and the names inside
    // them, and the file's own declarations would be noise in the one place they cannot be
    // written.
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
            let kind = completion_kind(s.kind);
            let detail = match &s.container {
                Some(owner) => format!("{owner}.{} — {}", s.name, s.detail),
                None => s.detail.clone(),
            };
            item(s.name, kind, detail)
        })
        .collect();

    // What the `#import` lines bring in, ranked between the file's own names and the
    // language's: more specific than a builtin, less than something declared right here.
    // For a Bevy shader this is most of what the file actually uses — `VertexOutput`,
    // `globals`, `apply_pbr_lighting` are all somebody else's declarations.
    let library = wgsl_library::for_file(file);
    let declared_here: std::collections::HashSet<&str> =
        out.iter().map(|i| i.label.as_str()).collect();
    let imported: Vec<CompletionItem> = library
        .in_scope(&text)
        .into_iter()
        .filter(|s| lower.is_empty() || s.name.to_ascii_lowercase().starts_with(&lower))
        // A name this file declares shadows the imported one, and the local declaration is
        // already in the list. Offering both is offering the same word twice with two
        // different signatures under it.
        .filter(|s| !declared_here.contains(s.name.as_str()))
        .map(|s| item(s.name.clone(), completion_kind(s.kind), imported_detail(s)))
        .collect();
    out.extend(imported);

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
        // Not declared here — which in a Bevy shader is the ordinary case, not the odd one:
        // `VertexOutput` and `apply_pbr_lighting` arrive through an `#import` and are
        // declared inside a crate the project depends on. The index knows where.
        return Some(imported_declaration(file, source, &name));
    };
    // The caret is already ON the declaration. There is nothing to go to *in this file*, and
    // answering with the span the caret is standing in is a jump to itself — which reads as
    // "go-to is broken" rather than as "you are there".
    //
    // It matters beyond the cosmetics: the empty answer is what lets the chain continue to the
    // framework extensions, and for a shader that is where the interesting jump lives. A
    // `struct SpiralHoverParams` in a shader has a Rust half — the `#[derive(ShaderType)]` it
    // has to match byte for byte — and until this returned nothing, that jump was unreachable
    // because this one always won first.
    if offset >= sym.start && offset <= sym.end {
        return Some(None);
    }
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


/// The completion list's kind tag for a declaration.
///
/// One mapping for the file's own symbols and the imported ones alike — otherwise the same
/// struct renders with two different icons depending on which file declared it.
fn completion_kind(kind: WgslSymbolKind) -> &'static str {
    match kind {
        WgslSymbolKind::Function | WgslSymbolKind::EntryPoint => "function",
        WgslSymbolKind::Struct | WgslSymbolKind::Alias => "class",
        WgslSymbolKind::Field => "field",
        WgslSymbolKind::Const | WgslSymbolKind::Override => "constant",
        WgslSymbolKind::Var => "variable",
    }
}

/// The hover card's kind tag. A different vocabulary from the completion list's — the FE
/// renders the two from different tables — so they are two functions rather than one with a
/// flag.
fn hover_kind(kind: WgslSymbolKind) -> &'static str {
    match kind {
        WgslSymbolKind::Function | WgslSymbolKind::EntryPoint => "method",
        WgslSymbolKind::Struct | WgslSymbolKind::Alias => "class",
        _ => "field",
    }
}

/// A completion/usage line for an imported declaration: what it is, and which module it
/// arrived from. The module is the half the name alone does not tell you, and in a Bevy
/// shader it is the thing you want to know.
fn imported_detail(s: &LibrarySymbol) -> String {
    let what = match &s.container {
        Some(owner) => format!("{owner}.{} — {}", s.name, s.detail),
        None => s.detail.clone(),
    };
    if what.is_empty() {
        s.module.clone()
    } else {
        format!("{what} · {}", s.module)
    }
}

/// Where an indexed module came from, for the line under a completion entry.
///
/// Derived from the path rather than recorded at index time, because the indexer is pure and
/// has no idea what a registry is. A registry checkout always contains the segment
/// `registry/src/<index>/<name>-<version>`, and that `<name>-<version>` is the most useful
/// thing to show: it names the crate AND the version, which is the half of "where is this
/// from" that a bare crate name leaves out.
fn module_origin(file: &str) -> String {
    let mut parts = file.split('/');
    while let Some(seg) = parts.next() {
        if seg != "registry" {
            continue;
        }
        if parts.next() != Some("src") {
            continue;
        }
        let _index = parts.next();
        if let Some(krate) = parts.next() {
            return krate.to_string();
        }
    }
    let base = file.rsplit('/').next().unwrap_or(file);
    format!("this project — {base}")
}

/// The declaration of a name that arrived through an `#import`.
///
/// Reports a position in a file the editor does not have open, which is exactly what the
/// index is for: line and column were resolved when the module was scanned, so a jump costs
/// a lookup rather than a disk read on every miss — and a miss is every identifier the file
/// does not declare, which is most of the ones a shader touches.
fn imported_declaration(file: &str, source: &str, name: &str) -> Option<DeclarationTarget> {
    let library = wgsl_library::for_file(file);
    let sym = library.resolve(source, name)?;
    Some(DeclarationTarget {
        file: sym.file.clone(),
        start: sym.start,
        end: sym.end,
        line: sym.line,
        col: sym.col,
        label: format!("{} — {}", sym.signature, sym.module),
    })
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
            kind: hover_kind(sym.kind).to_string(),
            container: sym.container.clone(),
            doc: doc_above(source, sym.start),
        }));
    }

    // An imported declaration outranks a builtin: if the file went to the trouble of
    // importing a name, that name is what it means.
    let library = wgsl_library::for_file(file);
    if let Some(sym) = library.resolve(source, &name) {
        return Some(Some(HoverInfo {
            signature: sym.signature.clone(),
            kind: hover_kind(sym.kind).to_string(),
            // The module, when the symbol is not a member of something — it is the answer to
            // "where does this come from", which is the question you hover an imported name
            // to ask.
            container: sym.container.clone().or_else(|| Some(sym.module.clone())),
            doc: sym.doc.clone(),
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

/// Completion for an `#import` line, or `None` when the caret is not on one.
///
/// Two things belong on an import line and they are not the same thing: a **module path**,
/// and a **name inside a module**. Which one the caret is asking for cannot be read off the
/// text — `bevy_pbr::forward_io` and `bevy_pbr::forward_io::VertexOutput` are the same shape
/// — so it is decided by asking the index whether everything before the last `::` is a
/// module. If it is, the tail is a name in it, and the names are what to offer.
///
/// Both are offered when both apply. A module that has items is still importable whole, and
/// deciding for the user which of the two they meant would be wrong about half the time.
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

    let library = wgsl_library::for_file(file);
    let mut out = Vec::new();

    // The path as written, with the enclosing package put back: inside `bevy_pbr::{ … }`
    // the caret only ever sees the tail, and the index is keyed by the whole path.
    let typed = match &package {
        Some(pkg) if !prefix.is_empty() => format!("{pkg}::{prefix}"),
        Some(pkg) => pkg.clone(),
        None => prefix.clone(),
    };

    // A name inside a module. The label carries everything the accept replaces — the caret's
    // word runs back through the `::`, so offering the bare name would eat the module.
    if let Some((module_path, tail)) = typed.rsplit_once("::") {
        if let Some(m) = library.module(module_path) {
            let head = &prefix[..prefix.len().saturating_sub(tail.len())];
            let tail_lower = tail.to_ascii_lowercase();
            for sym in &m.symbols {
                // A field cannot be imported on its own — it comes in with its struct.
                if sym.container.is_some() {
                    continue;
                }
                if !tail_lower.is_empty()
                    && !sym.name.to_ascii_lowercase().starts_with(&tail_lower)
                {
                    continue;
                }
                out.push(item(
                    format!("{head}{}", sym.name),
                    completion_kind(sym.kind),
                    format!("{} · {module_path}", sym.detail),
                ));
            }
        }
    }

    // Modules. The index first — it holds the project's own and every `bevy_*` module the
    // project actually resolved, which is both more complete and more current than any list
    // compiled into this binary can be.
    let mut offered: Vec<String> = Vec::new();
    let mut offer = |path: &str, detail: String, out: &mut Vec<CompletionItem>| {
        let label = match &package {
            // Inside `pkg::{ … }` only the tail is typed, so only the tail is offered.
            Some(pkg) => match path.strip_prefix(&format!("{pkg}::")) {
                Some(rest) => rest.to_string(),
                None => return,
            },
            None => path.to_string(),
        };
        if !lower.is_empty() && !label.to_ascii_lowercase().starts_with(&lower) {
            return;
        }
        if offered.iter().any(|o| o == &label) {
            return;
        }
        offered.push(label.clone());
        out.push(item(label, "module", detail));
    };

    for path in library.module_paths() {
        let detail = library.module(path).map(|m| module_origin(&m.file)).unwrap_or_default();
        offer(path, detail, &mut out);
    }

    // The compiled-in list last, and only for what the index did not have: on a project
    // cargo has never resolved it is the whole answer, and on one it has it adds nothing.
    for b in BEVY_IMPORTS {
        offer(b.path, b.detail.to_string(), &mut out);
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
        // `VertexOutput` comes in through the `#import`, and this toy source belongs to no
        // project — so there is no indexed module to jump into. Find-usages still answers,
        // and the label is where the user learns why there is no declaration in the list.
        // (With a real project the same caret DOES jump, into `forward_io.wgsl`; that path
        // is covered by `a_shader_jumps_into_the_module_it_imports`.)
        let at = caret("VertexOutput", 1);
        assert_eq!(declaration("s.wgsl", BEVY, at), Some(None));
        let usages = references("s.wgsl", BEVY, at).unwrap().expect("no usages");
        assert!(usages.target_label.contains("not declared in this file"));
    }
}

#[cfg(test)]
mod declaration_tests {
    use super::*;

    const SRC: &str = concat!(
        "struct Params {\n",
        "    sand_color: vec4<f32>,\n",
        "};\n\n",
        "@group(2) @binding(0)\n",
        "var<uniform> params: Params;\n\n",
        "fn use_it() -> vec4<f32> {\n",
        "    return params.sand_color;\n",
        "}\n",
    );

    #[test]
    fn a_use_still_goes_to_its_declaration() {
        let at = SRC.rfind("params.sand_color").unwrap() + 2;
        let target = declaration("s.wgsl", SRC, at).unwrap().expect("the var declaration");
        assert_eq!(&SRC[target.start..target.end], "params");
        assert!(target.start < at, "it jumped backwards to the declaration");
    }

    #[test]
    fn a_caret_on_the_declaration_answers_nothing_rather_than_itself() {
        // Both because a jump to where you already are reads as a broken feature, and because
        // the empty answer is what lets the chain reach the framework extensions — which is
        // where a shader struct's Rust half lives.
        let at = SRC.find("struct Params").unwrap() + "struct Pa".len();
        assert_eq!(declaration("s.wgsl", SRC, at), Some(None));

        let member = SRC.find("sand_color").unwrap() + 3;
        assert_eq!(declaration("s.wgsl", SRC, member), Some(None));
    }

    // ── the imported half: completion, hover and go-to across the `#import` ──────

    /// A throwaway project on disk: a `Cargo.toml` (so the root is found), a module that
    /// declares an import path, and a shader that imports from it.
    ///
    /// On disk rather than in memory because the whole point of this seam is that it walks a
    /// real tree — a test that handed the index its sources would be testing
    /// `bennu_wgsl::library`, which has its own.
    struct Fixture {
        root: std::path::PathBuf,
        shader: String,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bennu-wgsl-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("shaders")).unwrap();
            std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"toy\"\n").unwrap();
            std::fs::write(
                root.join("shaders/forward_io.wgsl"),
                concat!(
                    "#define_import_path toy::forward_io\n\n",
                    "// What the vertex stage hands to the fragment stage.\n",
                    "struct VertexOutput {\n",
                    "    @location(2) uv: vec2<f32>,\n",
                    "};\n\n",
                    "// Runs the lighting model.\n",
                    "fn apply_toy_lighting(c: vec4<f32>) -> vec4<f32> { return c; }\n",
                ),
            )
            .unwrap();
            let shader = root.join("shaders/mat.wgsl").to_string_lossy().replace('\\', "/");
            Self { root, shader }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const USES_IMPORT: &str = concat!(
        "#import toy::forward_io::{VertexOutput, apply_toy_lighting}\n\n",
        "@fragment\n",
        "fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {\n",
        "    return apply_toy_lighting(vec4<f32>(mesh.uv, 0.0, 1.0));\n",
        "}\n",
    );

    #[test]
    fn completion_offers_what_the_imports_brought_in() {
        let fx = Fixture::new("completion");
        let at = USES_IMPORT.find("apply_toy_lighting(vec4").unwrap() + "apply_toy".len();
        let items = completion(&fx.shader, at, Some(USES_IMPORT)).unwrap();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"apply_toy_lighting"), "{labels:?}");
        // The struct's fields come with it — `mesh.uv` is why it was imported.
        let all = completion(&fx.shader, USES_IMPORT.len() - 2, Some(USES_IMPORT)).unwrap();
        assert!(all.iter().any(|i| i.label == "uv"), "the struct's members come along");
    }

    #[test]
    fn a_shader_jumps_into_the_module_it_imports() {
        let fx = Fixture::new("goto");
        let at = USES_IMPORT.find("apply_toy_lighting(vec4").unwrap() + "apply_toy".len();
        let target = declaration(&fx.shader, USES_IMPORT, at).unwrap().expect("no declaration");
        assert!(target.file.ends_with("forward_io.wgsl"), "got {}", target.file);
        let text = std::fs::read_to_string(&target.file).unwrap();
        assert_eq!(&text[target.start..target.end], "apply_toy_lighting");
        // The position must be right in a file the editor has not opened — that is the whole
        // reason the index stores it.
        let line = text.lines().nth(target.line as usize - 1).unwrap();
        assert!(line.contains("fn apply_toy_lighting"), "line {} is {line:?}", target.line);
    }

    #[test]
    fn hover_on_an_imported_name_shows_its_real_signature_and_where_it_came_from() {
        let fx = Fixture::new("hover");
        let at = USES_IMPORT.find("apply_toy_lighting(vec4").unwrap() + "apply_toy".len();
        let card = hover(&fx.shader, USES_IMPORT, at).unwrap().expect("no hover card");
        assert!(card.signature.contains("fn apply_toy_lighting"), "got {:?}", card.signature);
        assert_eq!(card.container.as_deref(), Some("toy::forward_io"));
        assert!(
            card.doc.as_deref().unwrap_or("").contains("lighting model"),
            "the comment above the declaration IS its doc: {:?}",
            card.doc
        );
    }

    #[test]
    fn a_name_declared_here_beats_one_that_was_imported() {
        // Shadowing is legal and the local one is what runs. A card describing the imported
        // declaration would document code that is not being called.
        let fx = Fixture::new("shadow");
        let src = concat!(
            "#import toy::forward_io::{VertexOutput, apply_toy_lighting}\n\n",
            "// The local override.\n",
            "fn apply_toy_lighting(c: vec4<f32>) -> vec4<f32> { return c * 2.0; }\n\n",
            "fn f() { let x = apply_toy_lighting(vec4<f32>(1.0)); }\n",
        );
        let at = src.rfind("apply_toy_lighting").unwrap() + 3;
        let card = hover(&fx.shader, src, at).unwrap().expect("no hover card");
        assert!(card.doc.as_deref().unwrap_or("").contains("local override"), "{:?}", card.doc);
        let target = declaration(&fx.shader, src, at).unwrap().expect("no declaration");
        assert_eq!(target.file, fx.shader, "the jump stays in this file");
    }

    #[test]
    fn an_import_line_completes_the_names_inside_a_module() {
        // The gap this closes: the module path could always be completed, but the name after
        // it — the thing actually being imported — could not.
        let fx = Fixture::new("importitems");
        let src = "#import toy::forward_io::Vert";
        let items = completion(&fx.shader, src.len(), Some(src)).unwrap();
        assert!(
            items.iter().any(|i| i.label == "toy::forward_io::VertexOutput"),
            "got {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_import_line_offers_the_projects_own_modules() {
        let fx = Fixture::new("projectmods");
        let src = "#import toy::for";
        let items = completion(&fx.shader, src.len(), Some(src)).unwrap();
        assert!(
            items.iter().any(|i| i.label == "toy::forward_io"),
            "got {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}
