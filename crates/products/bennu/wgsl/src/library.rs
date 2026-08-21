//! The shader library — everything a composed shader can see that is not in its own file.
//!
//! A Bevy shader is a fragment. `#import bevy_pbr::pbr_functions::apply_pbr_lighting` puts a
//! function in scope whose body lives in another crate's source, and until that source is
//! read the editor is in the position of knowing the name exists and nothing else: no
//! completion for it, no signature on hover, nowhere to jump. Everything the file itself
//! declares is already answered by [`crate::symbols`]; this is the other half.
//!
//! ## What this module is, and what it deliberately is not
//!
//! It is an **index over source text handed in from outside** — a map from module path to
//! the declarations that module makes. It never opens a file. Finding Bevy's shaders is a
//! question about a machine (where cargo put a crate, which version the project resolved),
//! and that belongs to the host; the answer is a list of `(file, source)` pairs, and this
//! turns it into something a completion can answer from.
//!
//! It is **not** naga_oil. Nothing here composes a module, substitutes a `#{SHADER_DEF}` or
//! decides which `#ifdef` branch is live, and so nothing here makes a composed shader
//! compilable — [`crate::validate`] still declines those, for the reasons it gives. The
//! distinction is worth keeping sharp: composing is what you need to know whether a shader
//! *runs*, and an index is what you need to help somebody *write* it. The second is most of
//! the value and a fraction of the cost.
//!
//! ## Why the whole file is scanned rather than the imported name looked up
//!
//! Because a struct import is really a request for its fields. `#import
//! bevy_pbr::forward_io::VertexOutput` is written so that `mesh.uv` can be, and a resolver
//! that fetched only the declaration named `VertexOutput` would answer the question nobody
//! asked. The tolerant scanner already returns fields as members with their container, so
//! indexing whole modules costs nothing extra and makes the useful case work.

use std::collections::{BTreeMap, HashSet};

use crate::imports::{defined_path, parse_imports};
use crate::symbols::{doc_above, scan, signature_at, WgslSymbol, WgslSymbolKind};

/// 1-based line and column of a byte offset.
///
/// Columns are counted in **characters**, not bytes: an editor's caret is on the nth
/// character of the line, and a shader file with an accented word in a comment above a
/// declaration would otherwise report a column past where the name is.
fn line_col(text: &str, at: usize) -> (u32, u32) {
    let head = &text[..at.min(text.len())];
    let line = head.matches('\n').count() + 1;
    let col = head.rsplit('\n').next().unwrap_or(head).chars().count() + 1;
    (line as u32, col as u32)
}

/// One declaration made by a module other than the file being edited.
///
/// Carries its signature and doc comment **already extracted**, rather than a handle to the
/// source they came from. The index is built once per project and read on every keystroke,
/// so the trade is deliberate: pay the extraction at build time and let the sources go,
/// instead of holding a couple of megabytes of shader text to re-derive two strings from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySymbol {
    pub name: String,
    pub kind: WgslSymbolKind,
    /// What to show beside the name — the same detail [`WgslSymbol`] carries.
    pub detail: String,
    /// The struct this is a field of, if it is one.
    pub container: Option<String>,
    /// The declaration line, for a hover card.
    pub signature: String,
    /// The comment above the declaration. In WGSL that IS the documentation.
    pub doc: Option<String>,
    /// The module path that declares it (`bevy_pbr::forward_io`).
    pub module: String,
    /// Absolute path of the declaring file, forward-slashed like every other file the
    /// backend reports.
    pub file: String,
    /// Byte offsets of the NAME within `file`, so a go-to lands on the name.
    pub start: usize,
    pub end: usize,
    /// 1-based line and column of the name in `file`.
    ///
    /// Resolved here rather than by the caller, and this is the reason the index is worth
    /// having at all: a go-to has to report a position in a file that is **not open**, and
    /// the only other way to get one is to read that file back off disk on every jump.
    pub line: u32,
    pub col: u32,
}

/// One importable module: a file that declared a `#define_import_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderModule {
    pub path: String,
    pub file: String,
    pub symbols: Vec<LibrarySymbol>,
}

/// Every module a project can import, indexed by path.
#[derive(Debug, Clone, Default)]
pub struct ShaderLibrary {
    modules: BTreeMap<String, ShaderModule>,
}

impl ShaderLibrary {
    /// Index a set of `(file, source)` pairs.
    ///
    /// A file with no `#define_import_path` is skipped rather than indexed under its
    /// filename: naga_oil resolves imports by declared path only, so a file that has not
    /// declared one is not importable, and inventing a path for it would offer the user an
    /// import that cannot work.
    ///
    /// **First declaration of a path wins.** Two versions of the same crate can sit side by
    /// side in a registry cache, and both declare `bevy_pbr::forward_io`; the caller knows
    /// which one the project resolved, so it puts that one first and this does not
    /// second-guess it.
    pub fn index<I>(files: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut modules: BTreeMap<String, ShaderModule> = BTreeMap::new();
        for (file, source) in files {
            let Some(path) = defined_path(&source) else { continue };
            if modules.contains_key(&path) {
                continue;
            }
            let symbols = scan(&source)
                .into_iter()
                .map(|s: WgslSymbol| {
                    let (line, col) = line_col(&source, s.start);
                    LibrarySymbol {
                        signature: signature_at(&source, s.start),
                        doc: doc_above(&source, s.start),
                        module: path.clone(),
                        file: file.clone(),
                        name: s.name,
                        kind: s.kind,
                        detail: s.detail,
                        container: s.container,
                        start: s.start,
                        end: s.end,
                        line,
                        col,
                    }
                })
                .collect();
            modules.insert(path.clone(), ShaderModule { path, file, symbols });
        }
        Self { modules }
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn module(&self, path: &str) -> Option<&ShaderModule> {
        self.modules.get(path)
    }

    /// Every module path, in sorted order.
    pub fn module_paths(&self) -> impl Iterator<Item = &str> {
        self.modules.keys().map(String::as_str)
    }

    /// Whether a path names a module rather than an item inside one.
    ///
    /// The distinction cannot be made from the text: `bevy_pbr::forward_io` and
    /// `bevy_pbr::forward_io::VertexOutput` are the same shape, and only the index knows
    /// which of the two exists. This is why resolution lives here and parsing does not.
    pub fn is_module(&self, path: &str) -> bool {
        self.modules.contains_key(path)
    }

    /// Every declaration the file's `#import` lines bring within reach.
    ///
    /// In import order, deduplicated by (module, name, container) — a file that imports both
    /// a module and one item from it should not offer that item twice.
    pub fn in_scope(&self, source: &str) -> Vec<&LibrarySymbol> {
        let mut out: Vec<&LibrarySymbol> = Vec::new();
        let mut seen: HashSet<(&str, &str, Option<&str>)> = HashSet::new();

        for imported in parse_imports(source) {
            // `#import bevy_pbr::forward_io` — the whole module.
            if let Some(m) = self.modules.get(&imported.path) {
                for s in &m.symbols {
                    if seen.insert((s.module.as_str(), s.name.as_str(), s.container.as_deref())) {
                        out.push(s);
                    }
                }
                continue;
            }

            // `#import bevy_pbr::forward_io::VertexOutput` — one item, plus what it owns.
            let Some((module, name)) = imported.path.rsplit_once("::") else { continue };
            let Some(m) = self.modules.get(module) else { continue };
            for s in &m.symbols {
                let is_the_item = s.name == name && s.container.is_none();
                // A struct is imported so its members can be written. Bringing the fields
                // along is the whole point of naming the struct.
                let is_a_member = s.container.as_deref() == Some(name);
                if (is_the_item || is_a_member)
                    && seen.insert((s.module.as_str(), s.name.as_str(), s.container.as_deref()))
                {
                    out.push(s);
                }
            }
        }
        out
    }

    /// The declaration behind a bare name, as seen from `source`.
    ///
    /// Scoped to what the file imports rather than searched across the whole index: a name
    /// that resolves only because some other shader happens to import it is not in scope
    /// here, and jumping to it would be an answer to a question the file did not ask.
    pub fn resolve(&self, source: &str, name: &str) -> Option<&LibrarySymbol> {
        let scope = self.in_scope(source);
        // A top-level declaration beats a field of the same name: `VertexOutput` the struct
        // is what somebody means by `VertexOutput`, even if some struct also has a member
        // spelled that way.
        scope
            .iter()
            .find(|s| s.name == name && s.container.is_none())
            .or_else(|| scope.iter().find(|s| s.name == name))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for `bevy_pbr::forward_io` — the shape that matters is the
    /// `#define_import_path` on top and a struct whose fields somebody wants to write.
    const FORWARD_IO: &str = concat!(
        "#define_import_path bevy_pbr::forward_io\n",
        "\n",
        "// What the vertex stage hands to the fragment stage.\n",
        "struct VertexOutput {\n",
        "    @builtin(position) position: vec4<f32>,\n",
        "    @location(0) world_position: vec4<f32>,\n",
        "    @location(2) uv: vec2<f32>,\n",
        "};\n",
        "\n",
        "struct FragmentOutput {\n",
        "    @location(0) color: vec4<f32>,\n",
        "};\n",
    );

    const PBR_FUNCTIONS: &str = concat!(
        "#define_import_path bevy_pbr::pbr_functions\n",
        "\n",
        "// Runs the lighting model over a filled-in PbrInput.\n",
        "fn apply_pbr_lighting(in: PbrInput) -> vec4<f32> {\n",
        "    return in.material.base_color;\n",
        "}\n",
        "\n",
        "fn main_pass_post_lighting_processing(in: PbrInput, color: vec4<f32>) -> vec4<f32> {\n",
        "    return color;\n",
        "}\n",
    );

    /// A file with no `#define_import_path` — not importable, and so not indexed.
    const NOT_A_MODULE: &str = "fn helper() -> f32 { return 1.0; }\n";

    fn library() -> ShaderLibrary {
        ShaderLibrary::index([
            ("/bevy/forward_io.wgsl".to_string(), FORWARD_IO.to_string()),
            ("/bevy/pbr_functions.wgsl".to_string(), PBR_FUNCTIONS.to_string()),
            ("/game/loose.wgsl".to_string(), NOT_A_MODULE.to_string()),
        ])
    }

    #[test]
    fn only_files_that_declare_a_path_are_importable() {
        let lib = library();
        assert_eq!(lib.len(), 2);
        assert!(lib.is_module("bevy_pbr::forward_io"));
        assert!(!lib.is_module("bevy_pbr::forward_io::VertexOutput"));
    }

    #[test]
    fn importing_a_struct_brings_its_fields() {
        // The whole reason `VertexOutput` gets imported is so `mesh.uv` can be written. A
        // resolver that fetched only the struct would answer a question nobody asked.
        let lib = library();
        let src = "#import bevy_pbr::forward_io::VertexOutput\nfn f(m: VertexOutput) {}\n";
        let names: Vec<&str> = lib.in_scope(src).into_iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"VertexOutput"), "{names:?}");
        assert!(names.contains(&"uv"), "{names:?}");
        assert!(names.contains(&"world_position"), "{names:?}");
        // The other struct in the same module was not imported.
        assert!(!names.contains(&"FragmentOutput"), "{names:?}");
    }

    #[test]
    fn importing_a_module_brings_all_of_it() {
        let lib = library();
        let src = "#import bevy_pbr::pbr_functions\nfn f() {}\n";
        let names: Vec<&str> = lib.in_scope(src).into_iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"apply_pbr_lighting"), "{names:?}");
        assert!(names.contains(&"main_pass_post_lighting_processing"), "{names:?}");
    }

    #[test]
    fn a_name_resolves_to_its_declaration_with_the_file_and_the_offsets() {
        // This is what a go-to needs: not "it exists" but where, to the byte.
        let lib = library();
        let src = "#import bevy_pbr::pbr_functions::apply_pbr_lighting\nfn f() {}\n";
        let sym = lib.resolve(src, "apply_pbr_lighting").expect("not resolved");
        assert_eq!(sym.file, "/bevy/pbr_functions.wgsl");
        assert_eq!(&PBR_FUNCTIONS[sym.start..sym.end], "apply_pbr_lighting");
        assert!(sym.signature.contains("fn apply_pbr_lighting"), "{:?}", sym.signature);
        assert!(
            sym.doc.as_deref().unwrap_or("").contains("lighting model"),
            "the comment above a declaration IS its documentation: {:?}",
            sym.doc
        );
    }

    #[test]
    fn a_name_that_is_not_imported_does_not_resolve() {
        // Scoped to what the file asked for. Resolving across the whole index would make a
        // go-to answer for a name the shader cannot actually use.
        let lib = library();
        let src = "#import bevy_pbr::forward_io::VertexOutput\nfn f() {}\n";
        assert!(lib.resolve(src, "apply_pbr_lighting").is_none());
    }

    #[test]
    fn importing_a_module_and_an_item_from_it_offers_the_item_once() {
        let lib = library();
        let src = "#import bevy_pbr::pbr_functions\n#import bevy_pbr::pbr_functions::apply_pbr_lighting\n";
        let hits = lib
            .in_scope(src)
            .into_iter()
            .filter(|s| s.name == "apply_pbr_lighting")
            .count();
        assert_eq!(hits, 1);
    }

    #[test]
    fn the_first_declaration_of_a_path_wins() {
        // Two versions of a crate can sit side by side in a registry cache and both declare
        // the same module. The caller puts the resolved one first; this must not reorder.
        let lib = ShaderLibrary::index([
            ("/v2/forward_io.wgsl".to_string(), FORWARD_IO.to_string()),
            (
                "/v1/forward_io.wgsl".to_string(),
                "#define_import_path bevy_pbr::forward_io\nstruct VertexOutput { old: f32 };\n"
                    .to_string(),
            ),
        ]);
        assert_eq!(lib.module("bevy_pbr::forward_io").unwrap().file, "/v2/forward_io.wgsl");
    }

    #[test]
    fn a_resolved_symbol_carries_the_line_and_column_a_go_to_needs() {
        // The index exists so a jump into an unopened file needs no disk read. If these were
        // wrong the editor would open the right file at the wrong place, which is worse than
        // not jumping.
        let lib = library();
        let src = "#import bevy_pbr::forward_io::VertexOutput\n";
        let sym = lib.resolve(src, "VertexOutput").expect("not resolved");
        let line = FORWARD_IO.lines().nth(sym.line as usize - 1).unwrap();
        assert!(line.contains("struct VertexOutput"), "line {} is {line:?}", sym.line);
        assert_eq!(&line[sym.col as usize - 1..][.."VertexOutput".len()], "VertexOutput");
    }

    #[test]
    fn an_empty_library_answers_nothing_rather_than_panicking() {
        let lib = ShaderLibrary::default();
        assert!(lib.is_empty());
        assert!(lib.in_scope("#import bevy_pbr::forward_io::VertexOutput\n").is_empty());
        assert!(lib.resolve("#import bevy_pbr::forward_io::VertexOutput\n", "VertexOutput").is_none());
    }
}
