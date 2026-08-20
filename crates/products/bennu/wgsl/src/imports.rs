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
//! * **a catalogue of Bevy's own paths**, because those live inside the `bevy_*` crates'
//!   sources — under `~/.cargo/registry`, or wherever the dependency happens to be — and
//!   walking there from an editor is a guess about somebody's machine. The list below is
//!   short, stable, and covers what a material shader actually imports.

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

/// Bevy's own shader modules — the ones a material or a post-processing pass imports.
///
/// Curated rather than exhaustive on purpose. A shader is written with a handful of these,
/// and a list padded with every internal module Bevy happens to define would make the useful
/// entries harder to find while going stale just as fast.
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
}
