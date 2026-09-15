//! The Rust half of a shader: which material names which `.wgsl`, and what it promises to bind.
//!
//! ## Why this is a separate scan
//!
//! [`crate::items`] reads a file for its **ECS** shape and deliberately throws away two things
//! this needs: the *names* of a struct's fields (it keeps only their types, because a bundle is
//! identified by what it carries) and the attributes *on* those fields. A material is described
//! by exactly those two — `#[uniform(0)] pub params: SpiralHoverParams` says binding 0 is a
//! uniform holding that type — so widening the ECS scan to carry them would have every catalog
//! row pay for a fact only this reader wants.
//!
//! ## What it reads
//!
//! Three shapes, all by the same tolerant walk the rest of the crate uses:
//!
//! * `#[derive(AsBindGroup)] struct M { #[uniform(0)] f: T, … }` — the bind group.
//! * `impl Material for M { fn fragment_shader() -> ShaderRef { "shaders/x.wgsl".into() } }` —
//!   the link itself. Also `Material2d`, `UiMaterial` and `MaterialExtension`, which are the
//!   same declaration for a different pipeline.
//! * `#[derive(ShaderType)] struct P { a: Vec4, b: f32 }` — the type a uniform holds, whose
//!   layout has to match a `struct` in the shader byte for byte.
//!
//! ## Masked, except for the one thing that isn't
//!
//! The walk runs over [`crate::mask::mask`] like every other scan here, so a `#[uniform(0)]` in
//! a doc comment declares nothing. But the shader **path** is the body of a string literal,
//! which is exactly what the mask blanks — so the shape is found in the mask and the path is
//! read from the raw source at the same offsets. The mask preserves length precisely so that
//! this is possible.

use crate::items::{ident_at, is_ident_byte, last_segment, matching, skip_ws, split_top};

/// What a binding attribute says the resource is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Uniform,
    Texture,
    Sampler,
    Storage,
    /// A `#[texture]`/`#[sampler]` pair Bevy generates from `#[storage_texture]` and friends —
    /// kept as itself rather than folded into one of the above, so a row never claims a kind
    /// the attribute did not say.
    Other,
}

impl BindingKind {
    fn from_attr(name: &str) -> Option<BindingKind> {
        match name {
            "uniform" => Some(BindingKind::Uniform),
            "texture" | "texture_2d" | "texture_3d" | "texture_cube" => Some(BindingKind::Texture),
            "sampler" => Some(BindingKind::Sampler),
            "storage" | "storage_texture" => Some(BindingKind::Storage),
            "data" | "bind_group_data" => Some(BindingKind::Other),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BindingKind::Uniform => "uniform",
            BindingKind::Texture => "texture",
            BindingKind::Sampler => "sampler",
            BindingKind::Storage => "storage",
            BindingKind::Other => "binding",
        }
    }
}

/// One `#[uniform(0)] pub params: SpiralHoverParams`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBinding {
    pub index: u32,
    pub kind: BindingKind,
    /// The Rust field name.
    pub field: String,
    /// The field's type, verbatim.
    pub ty: String,
    /// Byte offset of the field name.
    pub offset: usize,
}

/// A `#[derive(AsBindGroup)]` struct — a material's resources, as Rust declares them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMaterial {
    pub name: String,
    /// Byte offset of the type name.
    pub offset: usize,
    pub bindings: Vec<RawBinding>,
}

/// A `#[derive(ShaderType)]` struct — the layout a uniform is laid out in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawShaderStruct {
    pub name: String,
    pub offset: usize,
    pub fields: Vec<RawField>,
}

/// One field of a `ShaderType` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawField {
    pub name: String,
    /// Verbatim Rust type — `Vec4`, `f32`, `[Vec2; 4]`.
    pub ty: String,
    pub offset: usize,
}

/// `fn fragment_shader() -> ShaderRef { "shaders/x.wgsl".into() }` inside an `impl … for M`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawShaderRef {
    /// The material type the `impl` is for.
    pub type_name: String,
    /// `fragment`, `vertex`, `prepass_fragment`, … — the method name with `_shader` removed.
    pub stage: String,
    /// The path as written in the literal.
    pub path: String,
    /// Byte offset of the literal's **first content byte** — where a go-to lands.
    pub offset: usize,
    /// Byte offset one past its last content byte.
    pub end: usize,
}

/// Everything one file said about materials.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShaderScan {
    pub materials: Vec<RawMaterial>,
    pub structs: Vec<RawShaderStruct>,
    pub refs: Vec<RawShaderRef>,
    /// `const STONE_SHADER: &str = "embedded://…";` — the string constants in this file, by name.
    ///
    /// Here because a `fragment_shader()` very often returns one rather than an inline literal:
    /// an engine crate embeds its shaders and names them once at the top of the file. A scan
    /// that only looked for a literal inside the method found nothing for every such material,
    /// which is most of them in a crate written that way.
    pub consts: Vec<(String, usize, usize)>,
}

/// The traits whose `fragment_shader()` names a shader. All four are the same declaration for a
/// different pipeline, and a project that mixes them (a 2D material beside a 3D one) is ordinary.
/// Keywords that may sit between an attribute and the item it belongs to.
const MODIFIERS: &[&str] = &["pub", "async", "unsafe", "const", "extern", "default", "static"];

const MATERIAL_TRAITS: &[&str] =
    &["Material", "Material2d", "UiMaterial", "MaterialExtension", "SpecializedMaterial"];

/// Read `masked` for material shapes, taking string bodies from `source`.
///
/// The two must be the same length — `masked` is [`crate::mask::mask`] of `source` — and every
/// offset returned is an offset into either.
pub fn scan(masked: &str, source: &str) -> ShaderScan {
    debug_assert_eq!(masked.len(), source.len(), "the mask must preserve length");
    let b = masked.as_bytes();
    let mut out = ShaderScan::default();
    let mut attrs: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i] == b'#' {
            // An attribute, kept until the item it belongs to arrives.
            let open = skip_ws(b, i + 1);
            let open = if b.get(open) == Some(&b'!') { skip_ws(b, open + 1) } else { open };
            match (b.get(open) == Some(&b'[')).then(|| matching(b, open)).flatten() {
                Some(close) => {
                    attrs.push(masked[open + 1..close].to_string());
                    i = close + 1;
                }
                None => i += 1,
            }
            continue;
        }
        let Some((word, after)) = ident_at(masked, i) else {
            i += 1;
            continue;
        };
        match word.as_str() {
            "const" | "static" => {
                i = read_const(masked, after, &mut out);
                attrs.clear();
            }
            "struct" => {
                i = read_struct(masked, after, &attrs, &mut out);
                attrs.clear();
            }
            "impl" => {
                i = read_impl(masked, source, after, &mut out);
                attrs.clear();
            }
            // A modifier sits BETWEEN the attributes and the item — `pub struct`, `pub(crate)
            // struct` — so it must not take them with it. Getting this wrong is silent: every
            // `pub` material in the project simply has no derives.
            w if MODIFIERS.contains(&w) => {
                let paren = skip_ws(b, after);
                i = match b.get(paren) {
                    Some(&b'(') => matching(b, paren).map_or(after, |c| c + 1),
                    _ => after,
                };
            }
            // Anything else takes the pending attributes with it: they belonged to whatever this
            // is, not to the next struct down the file.
            _ => {
                attrs.clear();
                i = after;
            }
        }
    }
    out
}

/// After `const` / `static`: a `&str` constant, remembered by name.
///
/// Only the ones with a string literal on the right: those are the only ones a `ShaderRef`
/// could be. Anything else is skipped rather than recorded as an empty path.
fn read_const(masked: &str, at: usize, out: &mut ShaderScan) -> usize {
    let b = masked.as_bytes();
    let start = skip_ws(b, at);
    let start = match ident_at(masked, start) {
        // `static mut` / `const fn` — the modifier, then the name.
        Some((w, after)) if w == "mut" || w == "fn" => skip_ws(b, after),
        _ => start,
    };
    let Some((name, after)) = ident_at(masked, start) else { return start.max(at) };
    let end = b[after..].iter().position(|&c| c == b';').map_or(b.len(), |n| after + n);
    match first_literal(masked, after, end) {
        Some((s, e)) => {
            out.consts.push((name, s, e));
            end + 1
        }
        None => end.min(b.len()),
    }
}

/// After `struct`: a material, a shader-type layout, both, or neither.
fn read_struct(masked: &str, at: usize, attrs: &[String], out: &mut ShaderScan) -> usize {
    let b = masked.as_bytes();
    let start = skip_ws(b, at);
    let Some((name, after)) = ident_at(masked, start) else { return start.max(at) };
    let derives = derived(attrs);
    let is_material = derives.iter().any(|d| d == "AsBindGroup");
    let is_layout = derives.iter().any(|d| d == "ShaderType");
    // Generics, then the body.
    let mut j = skip_ws(b, after);
    if b.get(j) == Some(&b'<') {
        j = matching(b, j).map_or(j, |c| skip_ws(b, c + 1));
    }
    let Some(close) = (if b.get(j) == Some(&b'{') { matching(b, j) } else { None }) else {
        return j;
    };
    if !is_material && !is_layout {
        return close + 1;
    }
    let fields = read_fields(masked, j + 1, close);
    if is_material {
        out.materials.push(RawMaterial {
            name: name.clone(),
            offset: start,
            bindings: fields.iter().filter_map(binding_of).collect(),
        });
    }
    if is_layout {
        out.structs.push(RawShaderStruct {
            name,
            offset: start,
            fields: fields
                .iter()
                .map(|f| RawField { name: f.name.clone(), ty: f.ty.clone(), offset: f.offset })
                .collect(),
        });
    }
    close + 1
}

/// One field of a struct body, with whatever attributes sat above it.
struct ScannedField {
    name: String,
    ty: String,
    offset: usize,
    attrs: Vec<String>,
}

/// Walk a struct body between `from` and `close`, at depth 0 only.
fn read_fields(masked: &str, from: usize, close: usize) -> Vec<ScannedField> {
    let b = masked.as_bytes();
    let mut out = Vec::new();
    let mut attrs: Vec<String> = Vec::new();
    let mut i = from;
    while i < close {
        if b[i].is_ascii_whitespace() || b[i] == b',' {
            i += 1;
            continue;
        }
        if b[i] == b'#' {
            let open = skip_ws(b, i + 1);
            match (b.get(open) == Some(&b'[')).then(|| matching(b, open)).flatten() {
                Some(end) if end < close => {
                    attrs.push(masked[open + 1..end].to_string());
                    i = end + 1;
                }
                _ => i += 1,
            }
            continue;
        }
        // `pub` / `pub(crate)` before the name.
        if let Some((word, after)) = ident_at(masked, i) {
            if word == "pub" {
                let paren = skip_ws(b, after);
                i = match b.get(paren) {
                    Some(&b'(') => matching(b, paren).map_or(after, |c| c + 1),
                    _ => after,
                };
                continue;
            }
            let name_start = i;
            let colon = skip_ws(b, after);
            if b.get(colon) != Some(&b':') {
                // Not `name: ty` — a tuple field, or something this walk does not model. Its
                // attributes go with it rather than sliding onto the next field.
                attrs.clear();
                i = after;
                continue;
            }
            let (ty, next) = read_field_type(masked, colon + 1, close);
            out.push(ScannedField {
                name: word,
                ty,
                offset: name_start,
                attrs: std::mem::take(&mut attrs),
            });
            i = next;
            continue;
        }
        i += 1;
    }
    out
}

/// A field's type: from after the `:` to the `,` that ends it, honouring nesting.
fn read_field_type(masked: &str, from: usize, close: usize) -> (String, usize) {
    let b = masked.as_bytes();
    let mut depth = 0i32;
    let mut i = from;
    while i < close {
        match b[i] {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b',' if depth <= 0 => break,
            b'-' if b.get(i + 1) == Some(&b'>') => i += 1,
            _ => {}
        }
        i += 1;
    }
    (masked[from..i].trim().to_string(), i)
}

/// The binding a field's attributes declare, if they declare one.
fn binding_of(field: &ScannedField) -> Option<RawBinding> {
    for attr in &field.attrs {
        let attr = attr.trim();
        let (head, rest) = match attr.find('(') {
            Some(p) => (&attr[..p], &attr[p + 1..attr.rfind(')').unwrap_or(attr.len())]),
            None => continue,
        };
        let Some(kind) = BindingKind::from_attr(head.trim()) else { continue };
        // The index is the first argument; the rest are options (`visibility(fragment)`,
        // `dimension(…)`) this side has no opinion about.
        let first = split_top(rest, b',').into_iter().next().unwrap_or_default();
        let Ok(index) = first.trim().parse::<u32>() else { continue };
        return Some(RawBinding {
            index,
            kind,
            field: field.name.clone(),
            ty: field.ty.clone(),
            offset: field.offset,
        });
    }
    None
}

/// The trait names inside every `derive(…)` in `attrs`.
fn derived(attrs: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for attr in attrs {
        let attr = attr.trim();
        let Some(rest) = attr.strip_prefix("derive") else { continue };
        let rest = rest.trim();
        let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else { continue };
        out.extend(split_top(inner, b',').into_iter().map(|t| last_segment(&t)));
    }
    out
}

/// After `impl`: an `impl Material for M` body, read for its shader methods.
fn read_impl(masked: &str, source: &str, at: usize, out: &mut ShaderScan) -> usize {
    let b = masked.as_bytes();
    let head_end =
        b[at..].iter().position(|&c| c == b'{' || c == b';').map_or(b.len(), |n| at + n);
    let header = &masked[at..head_end];
    let Some((tr, ty)) = header.split_once(" for ") else { return head_end };
    let trait_name = last_segment(tr.split_whitespace().next_back().unwrap_or("").split('<').next().unwrap_or(""));
    if !MATERIAL_TRAITS.contains(&trait_name.as_str()) {
        return head_end;
    }
    let type_name =
        last_segment(ty.split_whitespace().next().unwrap_or("").split('<').next().unwrap_or(""));
    if type_name.is_empty() || b.get(head_end) != Some(&b'{') {
        return head_end;
    }
    let Some(body_end) = matching(b, head_end) else { return head_end };
    for found in shader_methods(masked, head_end + 1, body_end) {
        let (offset, end) = match found.literal {
            Some(span) => span,
            // The method returned a bare identifier. If this file declares it as a string
            // constant, the path is that constant's — and the span stays the CONSTANT's, so a
            // go-to lands on the path itself rather than on the name that stands for it.
            None => match out.consts.iter().find(|(n, _, _)| Some(n.as_str()) == found.returns.as_deref()) {
                Some((_, s, e)) => (*s, *e),
                None => continue,
            },
        };
        out.refs.push(RawShaderRef {
            type_name: type_name.clone(),
            stage: found.stage,
            // The mask blanked the literal's body; the path is the raw bytes at the same span.
            path: source[offset..end].to_string(),
            offset,
            end,
        });
    }
    body_end + 1
}

/// What one `fn <stage>_shader()` said.
struct ShaderMethod {
    stage: String,
    /// The span of the first string literal in its body, when it has one.
    literal: Option<(usize, usize)>,
    /// The bare identifier it returns instead — a constant declared elsewhere in the file.
    returns: Option<String>,
}

/// Every `fn <stage>_shader()` in an impl body, and what it names the shader with.
///
/// The literal is found in the **mask**, where a `"` is still a `"` and everything between them
/// is a space — which is precisely what makes "the first literal after this `fn`" a safe thing
/// to look for: a `//` comment mentioning a path was blanked along with it.
fn shader_methods(masked: &str, from: usize, to: usize) -> Vec<ShaderMethod> {
    let b = masked.as_bytes();
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        if !is_word(b, i, b"fn") {
            i += 1;
            continue;
        }
        let name_at = skip_ws(b, i + 2);
        let Some((name, after)) = ident_at(masked, name_at) else {
            i = name_at.max(i + 2);
            continue;
        };
        let Some(stage) = name.strip_suffix("_shader") else {
            i = after;
            continue;
        };
        // The body: from the `{` that opens it to its match, so a literal in the *next* method
        // cannot be read as this one's.
        let Some(open) = masked[after..to].find('{').map(|k| after + k) else { break };
        let Some(body_end) = matching(b, open) else { break };
        let literal = first_literal(masked, open + 1, body_end);
        let returns = literal.is_none().then(|| bare_return(masked, open + 1, body_end)).flatten();
        if literal.is_some() || returns.is_some() {
            out.push(ShaderMethod { stage: stage.to_string(), literal, returns });
        }
        i = body_end + 1;
    }
    out
}

/// The span *inside* the first `"…"` between `from` and `to`.
fn first_literal(masked: &str, from: usize, to: usize) -> Option<(usize, usize)> {
    let b = masked.as_bytes();
    let open = b[from..to].iter().position(|&c| c == b'"')? + from;
    let close = b[open + 1..to].iter().position(|&c| c == b'"')? + open + 1;
    Some((open + 1, close))
}

/// The first identifier in a method body that only names a constant — `STONE_SHADER.into()`.
///
/// Deliberately the FIRST: the body of a `fragment_shader()` that names a constant is one
/// expression long, and a body with control flow in it is one this side declines to reason
/// about rather than guessing which branch runs.
fn bare_return(masked: &str, from: usize, to: usize) -> Option<String> {
    let b = masked.as_bytes();
    let mut i = from;
    while i < to {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let (word, after) = ident_at(masked, i)?;
        if word == "return" {
            i = after;
            continue;
        }
        // A path (`self::STONE`) keeps its last segment; anything that is not a plain name
        // followed by `.` or `;` is not a constant this can resolve.
        let mut end = after;
        while masked[end..].starts_with("::") {
            let (next, a) = ident_at(masked, end + 2)?;
            let _ = next;
            end = a;
        }
        return Some(crate::items::last_segment(&masked[i..end]));
    }
    None
}

fn is_word(b: &[u8], at: usize, word: &[u8]) -> bool {
    if !b[at..].starts_with(word) {
        return false;
    }
    let before = at == 0 || !is_ident_byte(b[at - 1]);
    let after = at + word.len();
    before && (after >= b.len() || !is_ident_byte(b[after]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::mask;

    pub(super) const SRC: &str = r#"
use bevy::prelude::*;

/// A doc comment showing `#[derive(AsBindGroup)] struct Ghost { }` — declares nothing.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SpiralHoverMaterial {
    #[uniform(0)]
    pub params: SpiralHoverParams,
    #[texture(1)]
    #[sampler(2)]
    pub grain: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SpiralHoverParams {
    pub sand_color: Vec4,
    pub dark_color: Vec4,
    pub spiral_speed: f32,
}

impl Material for SpiralHoverMaterial {
    fn fragment_shader() -> ShaderRef {
        // "shaders/decoy.wgsl" — a comment, not the answer.
        "shaders/spiral_hover.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "shaders/spiral_vertex.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode { AlphaMode::Blend }
}
"#;

    fn scanned() -> ShaderScan {
        scan(&mask(SRC), SRC)
    }

    #[test]
    fn a_bind_group_is_read_with_its_indices_and_kinds() {
        let s = scanned();
        assert_eq!(s.materials.len(), 1);
        let m = &s.materials[0];
        assert_eq!(m.name, "SpiralHoverMaterial");
        assert_eq!(&SRC[m.offset..m.offset + m.name.len()], "SpiralHoverMaterial");
        let kinds: Vec<(u32, BindingKind, &str)> =
            m.bindings.iter().map(|b| (b.index, b.kind, b.field.as_str())).collect();
        // The `#[sampler(2)]` sits under `#[texture(1)]` on the same field: the first attribute
        // that names a binding wins, which is the one Bevy reads for the texture itself.
        assert_eq!(
            kinds,
            vec![(0, BindingKind::Uniform, "params"), (1, BindingKind::Texture, "grain")]
        );
        assert_eq!(m.bindings[0].ty, "SpiralHoverParams");
    }

    #[test]
    fn a_shader_type_layout_keeps_its_field_names_and_types() {
        let s = scanned();
        let p = s.structs.iter().find(|s| s.name == "SpiralHoverParams").unwrap();
        let fields: Vec<(&str, &str)> =
            p.fields.iter().map(|f| (f.name.as_str(), f.ty.as_str())).collect();
        assert_eq!(
            fields,
            vec![("sand_color", "Vec4"), ("dark_color", "Vec4"), ("spiral_speed", "f32")]
        );
    }

    #[test]
    fn each_stage_finds_its_own_shader_path() {
        let s = scanned();
        let by_stage: Vec<(&str, &str)> =
            s.refs.iter().map(|r| (r.stage.as_str(), r.path.as_str())).collect();
        assert_eq!(
            by_stage,
            vec![
                ("fragment", "shaders/spiral_hover.wgsl"),
                ("vertex", "shaders/spiral_vertex.wgsl"),
            ]
        );
        assert!(s.refs.iter().all(|r| r.type_name == "SpiralHoverMaterial"));
    }

    #[test]
    fn the_path_span_points_at_the_path() {
        let s = scanned();
        let r = &s.refs[0];
        assert_eq!(&SRC[r.offset..r.end], "shaders/spiral_hover.wgsl");
    }

    #[test]
    fn a_commented_out_path_is_not_the_answer() {
        // The decoy is inside a `//` comment, which the mask blanked — including its quotes.
        let s = scanned();
        assert!(s.refs.iter().all(|r| r.path != "shaders/decoy.wgsl"));
    }

    #[test]
    fn a_doc_comment_declares_no_material() {
        let s = scanned();
        assert!(s.materials.iter().all(|m| m.name != "Ghost"));
    }

    #[test]
    fn a_struct_with_neither_derive_is_not_this_modules_business() {
        let s = scan(&mask("struct Plain { a: u32 }"), "struct Plain { a: u32 }");
        assert!(s.materials.is_empty() && s.structs.is_empty());
    }

    #[test]
    fn an_impl_of_something_else_names_no_shader() {
        let src = "impl Default for Thing { fn fragment_shader() -> S { \"x.wgsl\".into() } }";
        assert!(scan(&mask(src), src).refs.is_empty());
    }
}

