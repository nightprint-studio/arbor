//! naga_oil's imports — the lines that make a Bevy shader a fragment of something larger.
//!
//! Not WGSL. `#import`, `#define_import_path` and `#{SHADER_DEF}` are directives to the
//! **composer** that stitches shaders together before wgpu ever sees them, which is why the
//! compiler side of this crate refuses to compile a file that has them. This module is the
//! other half of that bargain: if Bennu will not check those files, it can at least help
//! write them.
//!
//! Two sources of truth for what can be imported, and they cover different things:
//!
//! * **the project itself** — every `#define_import_path` in it. Always right, and the only
//!   way to know about the modules the user wrote.
//! * **Bevy's own modules**, read from the `bevy_*` crate sources the project actually
//!   resolved. Those live under `~/.cargo/registry`, which sounds like a guess about
//!   somebody's machine and is not one: `Cargo.lock` names the exact version, and the
//!   registry lays it out at a path derived from the name and that version. The host does
//!   the looking (see `bennu-be`'s `wgsl_library`); [`crate::library`] turns what it finds
//!   into an index.
//!
//! [`BEVY_IMPORTS`] below is the **floor**, not the answer: what to offer before a project
//! has been resolved, or when Bevy came from a git or path dependency that is not laid out
//! the way the registry is. When the index has the real modules it supersedes this list,
//! because the list is a guess about a version and the index is that version.

/// The path a file declares itself as, from its `#define_import_path`, if it has one.
pub fn defined_path(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let t = line.trim();
        let rest = t.strip_prefix("#define_import_path")?;
        let name = rest.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// What the caret is in the middle of typing, on an import line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportContext {
    /// A module path (`bevy_pbr::mesh_view_bind`). The prefix typed so far.
    Module { prefix: String, start: usize },
    /// An item inside a braced import (`#import bevy_pbr::{forward_io::Vert`). The prefix,
    /// and the package it is being taken from.
    Item { prefix: String, start: usize, package: String },
}

/// Where an identifier-ish prefix starts, walking back from `at`.
fn prefix_start(bytes: &[u8], at: usize) -> usize {
    let mut start = at.min(bytes.len());
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b':' {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

/// Whether `offset` sits inside an `#import` — and if so, what is being completed.
///
/// Scans backwards for the directive rather than forwards from it, because the braced form
/// spans lines: by the time the caret is on `forward_io::Vert` the `#import` is three lines
/// up, and a line-local test would conclude this is ordinary code.
pub fn context_at(source: &str, offset: usize) -> Option<ImportContext> {
    let head = &source[..offset.min(source.len())];
    let directive = head.rfind("#import")?;
    let between = &head[directive..];

    // Anything after the directive that ends it: a closing brace at depth zero, or — for the
    // unbraced form — a newline.
    let mut depth = 0usize;
    for (i, ch) in between.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && i + 1 < between.len() {
                    return None; // the import closed before the caret
                }
            }
            '\n' if depth == 0 && i > 0 => return None, // an unbraced import ends at the line
            _ => {}
        }
    }

    let bytes = source.as_bytes();
    let start = prefix_start(bytes, offset);
    let prefix = source[start..offset.min(source.len())].to_string();

    if depth > 0 {
        // Inside the braces: the package is the word between `#import` and `::{`.
        let package = between
            .trim_start_matches("#import")
            .split("::")
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(ImportContext::Item { prefix, start, package });
    }
    Some(ImportContext::Module { prefix, start })
}

/// One offered import path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportPath {
    pub path: &'static str,
    pub detail: &'static str,
}

const fn i(path: &'static str, detail: &'static str) -> ImportPath {
    ImportPath { path, detail }
}

/// Bevy's own shader modules — the fallback list, for when the real sources are not reachable.
///
/// Curated rather than exhaustive on purpose: a shader is written with a handful of these,
/// and this exists to be useful on a project cargo has never resolved. Once
/// [`crate::library::ShaderLibrary`] has indexed the crate sources, it wins — it knows the
/// modules this list forgets and the ones this list still names two releases after they were
/// renamed.
pub const BEVY_IMPORTS: &[ImportPath] = &[
    i("bevy_pbr::forward_io", "VertexOutput, FragmentOutput"),
    i("bevy_pbr::mesh_view_bindings", "view, globals, lights"),
    i("bevy_pbr::mesh_bindings", "the mesh's own uniforms"),
    i("bevy_pbr::mesh_functions", "world-space transforms"),
    i("bevy_pbr::pbr_types", "StandardMaterial's structs"),
    i("bevy_pbr::pbr_bindings", "the standard material's textures"),
    i("bevy_pbr::pbr_functions", "the PBR lighting entry points"),
    i("bevy_pbr::pbr_fragment", "pbr_input_from_standard_material"),
    i("bevy_pbr::utils", "hsv/rgb, random, coordinate helpers"),
    i("bevy_pbr::view_transformations", "clip / view / world conversions"),
    i("bevy_pbr::shadows", "shadow sampling"),
    i("bevy_pbr::clustered_forward", "light clustering"),
    i("bevy_pbr::prepass_utils", "depth / normal prepass samplers"),
    i("bevy_render::view", "View, the camera's uniforms"),
    i("bevy_render::globals", "Globals — time, delta, frame count"),
    i("bevy_render::maths", "matrix and projection helpers"),
    i("bevy_render::color_operations", "colour space conversions"),
    i("bevy_sprite::mesh2d_vertex_output", "VertexOutput, for 2D"),
    i("bevy_sprite::mesh2d_view_bindings", "view and globals, for 2D"),
    i("bevy_sprite::mesh2d_functions", "2D transforms"),
    i("bevy_sprite::mesh2d_bindings", "the 2D mesh's uniforms"),
    i("bevy_core_pipeline::fullscreen_vertex_shader", "FullscreenVertexOutput"),
    i("bevy_core_pipeline::tonemapping", "tone mapping functions"),
    i("bevy_ui::ui_vertex_output", "UiVertexOutput"),
];

/// One path brought into a file by an `#import`, flattened out of whatever nesting the
/// directive was written with.
///
/// Deliberately does **not** say whether the path names a module or an item inside one:
/// `bevy_pbr::forward_io` and `bevy_pbr::forward_io::VertexOutput` are the same shape, and
/// only an index of what exists can tell them apart. Parsing reports what was written;
/// [`crate::library::ShaderLibrary`] decides what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedPath {
    /// The full path, with every enclosing prefix applied.
    pub path: String,
    /// The local name, when the import renames it (`… as view_bindings`).
    pub alias: Option<String>,
}

/// The index of the `}` matching the `{` at `open`, if the braces balance.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split on commas that are not inside braces.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut from = 0usize;
    for (i, ch) in text.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&text[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[from..]);
    out
}

fn join(prefix: &str, tail: &str) -> String {
    match (prefix.is_empty(), tail.is_empty()) {
        (true, _) => tail.to_string(),
        (_, true) => prefix.to_string(),
        _ => format!("{prefix}::{tail}"),
    }
}

/// Expand one comma-free segment, which may itself open a brace group.
fn expand(segment: &str, prefix: &str, out: &mut Vec<ImportedPath>) {
    let t = segment.trim();
    // `#import "shaders/util.wgsl"` — the file form. It names no module path, so there is
    // nothing here to resolve against an index.
    if t.is_empty() || t.starts_with('"') {
        return;
    }
    if let Some(open) = t.find('{') {
        let head = t[..open].trim().trim_end_matches(':');
        let close = matching_brace(t, open).unwrap_or(t.len());
        let inner = &t[open + 1..close.min(t.len())];
        let joined = join(prefix, head);
        for part in split_top_level(inner) {
            expand(part, &joined, out);
        }
        return;
    }
    let (path, alias) = match t.split_once(" as ") {
        Some((p, a)) => (p.trim(), Some(a.trim().to_string())),
        None => (t, None),
    };
    if path.is_empty() {
        return;
    }
    out.push(ImportedPath { path: join(prefix, path), alias });
}

/// How far an `#import` directive reaches, measured from just after the keyword.
///
/// Two terminators, because there are two forms: the braced one ends at the `}` that closes
/// it and may span lines, the plain one ends at the newline. Getting this wrong in the
/// permissive direction swallows the shader that follows.
fn extent(rest: &str) -> usize {
    let mut depth = 0usize;
    for (i, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            '\n' if depth == 0 => return i,
            _ => {}
        }
    }
    rest.len()
}

/// Every path a file imports.
///
/// Comments are blanked first, so an `#import` inside a commented-out block does not put
/// anything in scope — a shader mid-edit very often has one.
pub fn parse_imports(source: &str) -> Vec<ImportedPath> {
    let blanked = crate::symbols::blank_comments(source);
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = blanked[from..].find("#import") {
        let at = from + rel;
        // Only at the start of a line: `x = "#import"` is a string, not a directive.
        let line_start = blanked[..at].rfind('\n').map_or(0, |n| n + 1);
        let is_directive = blanked[line_start..at].trim().is_empty();
        let rest = &blanked[at + "#import".len()..];
        let len = extent(rest);
        if is_directive {
            expand(&rest[..len], "", &mut out);
        }
        from = at + "#import".len() + len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_declares_its_own_path() {
        assert_eq!(
            defined_path("#define_import_path my_game::noise\n\nfn f() {}\n").as_deref(),
            Some("my_game::noise")
        );
        assert_eq!(defined_path("fn f() {}\n"), None);
    }

    #[test]
    fn the_unbraced_form_completes_a_module() {
        let src = "#import bevy_pbr::mesh_view";
        let ctx = context_at(src, src.len()).unwrap();
        assert_eq!(
            ctx,
            ImportContext::Module { prefix: "bevy_pbr::mesh_view".into(), start: 8 }
        );
    }

    #[test]
    fn the_braced_form_completes_an_item_and_knows_its_package() {
        // The caret is three lines below the directive — a line-local test would call this
        // ordinary code, which is exactly the case this has to get right.
        let src = "#import bevy_pbr::{\n    forward_io::VertexOutput,\n    mesh_view_bind";
        let ctx = context_at(src, src.len()).unwrap();
        match ctx {
            ImportContext::Item { prefix, package, .. } => {
                assert_eq!(prefix, "mesh_view_bind");
                assert_eq!(package, "bevy_pbr");
            }
            other => panic!("expected an item completion, got {other:?}"),
        }
    }

    #[test]
    fn code_after_the_import_is_not_an_import() {
        let src = "#import bevy_pbr::forward_io::VertexOutput\n\nfn f() { let x = 1; }\n";
        assert_eq!(context_at(src, src.len() - 3), None);
    }

    #[test]
    fn code_after_a_closed_braced_import_is_not_an_import() {
        let src = "#import bevy_pbr::{forward_io::VertexOutput}\n\nfn f() { let xy";
        assert_eq!(context_at(src, src.len()), None);
    }

    // ── parse_imports ────────────────────────────────────────────────────────

    fn paths(src: &str) -> Vec<String> {
        parse_imports(src).into_iter().map(|i| i.path).collect()
    }

    #[test]
    fn the_plain_form_is_one_path() {
        assert_eq!(
            paths("#import bevy_pbr::forward_io::VertexOutput\nfn f() {}\n"),
            vec!["bevy_pbr::forward_io::VertexOutput"]
        );
    }

    #[test]
    fn the_braced_form_gets_the_prefix_applied_to_every_item() {
        // Exactly how every shader in the project opens.
        let src = "#import bevy_pbr::{\n    forward_io::VertexOutput,\n    mesh_view_bindings::globals,\n}\n";
        assert_eq!(
            paths(src),
            vec![
                "bevy_pbr::forward_io::VertexOutput",
                "bevy_pbr::mesh_view_bindings::globals",
            ]
        );
    }

    #[test]
    fn braces_nest() {
        // `metal.wgsl` is written this way: a group inside a group.
        let src = "#import bevy_pbr::{\n    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},\n    forward_io::{VertexOutput, FragmentOutput},\n}\n";
        assert_eq!(
            paths(src),
            vec![
                "bevy_pbr::pbr_functions::apply_pbr_lighting",
                "bevy_pbr::pbr_functions::main_pass_post_lighting_processing",
                "bevy_pbr::forward_io::VertexOutput",
                "bevy_pbr::forward_io::FragmentOutput",
            ]
        );
    }

    #[test]
    fn a_module_import_carries_no_item() {
        // Nothing here says whether the last segment is a module or a name in one — that is
        // the index's job, and the parser must not pretend to know.
        assert_eq!(paths("#import bevy_pbr::mesh_view_bindings\n"), vec!["bevy_pbr::mesh_view_bindings"]);
    }

    #[test]
    fn an_alias_is_kept_beside_the_path() {
        let one = parse_imports("#import bevy_pbr::mesh_view_bindings as view_bindings\n");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, "bevy_pbr::mesh_view_bindings");
        assert_eq!(one[0].alias.as_deref(), Some("view_bindings"));
    }

    #[test]
    fn the_directive_does_not_swallow_the_shader_after_it() {
        // The failure that matters: an extent that runs past the newline takes the whole
        // file with it, and every name in the shader becomes an import.
        let src = "#import bevy_pbr::forward_io::VertexOutput\n\nstruct P { x: f32 };\nfn f() {}\n";
        assert_eq!(paths(src), vec!["bevy_pbr::forward_io::VertexOutput"]);
    }

    #[test]
    fn a_commented_out_import_puts_nothing_in_scope() {
        // Extremely common mid-edit, and a resolver fooled by it reports names the shader
        // cannot actually use.
        assert!(paths("// #import bevy_pbr::forward_io::VertexOutput\nfn f() {}\n").is_empty());
        assert!(paths("/*\n#import bevy_pbr::pbr_functions::apply_pbr_lighting\n*/\n").is_empty());
    }

    #[test]
    fn the_file_form_names_no_module() {
        assert!(paths("#import \"shaders/util.wgsl\"\n").is_empty());
    }

    #[test]
    fn several_directives_all_count() {
        let src = "#import bevy_pbr::forward_io::VertexOutput\n#import bevy_render::globals::Globals\n";
        assert_eq!(paths(src).len(), 2);
    }
}
