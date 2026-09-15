//! Where a material and its shader are checked against each other.
//!
//! ## Why this is worth doing at all
//!
//! Because the compiler never does it. A `#[derive(ShaderType)]` struct in Rust and a `struct`
//! in WGSL are two descriptions of the same block of bytes, written in two files, in two
//! languages, and **nothing** verifies that they agree. Get a field out of order, or write
//! `f32` where the shader says `vec4<f32>`, and everything still builds — the uniform is just
//! quietly wrong, and what you see is a colour that is not the colour you asked for. Every
//! shader author has spent an afternoon on that bug.
//!
//! It is the same shape as the checks Bennu already runs across a JSP and a Struts config: two
//! files, one relationship, no compiler standing over it.
//!
//! ## What it will and will not claim
//!
//! Held to the same standard as the rest of the framework seam — under-report rather than risk
//! a false positive:
//!
//! * A type it does not recognise on **either** side ends the comparison for that field. There
//!   is no "probably fine" here: a wrong claim about a layout is worse than silence, because
//!   silence is what the user already has.
//! * A shader whose struct is not in the file is not a mismatch. It may arrive through an
//!   `#import`, and this side does not resolve naga_oil's composition (see `bennu-wgsl`).
//! * The WGSL→Rust direction is checked only for bindings in the **material's** bind group.
//!   The view group is not the material's to declare, and a shader def this side cannot resolve
//!   is not assumed to be one.
//! * A project with **no shader assets at all** is told nothing about missing ones. An engine
//!   crate declares the material and the game that depends on it ships the `.wgsl` — the
//!   arrangement this was written against, in fact — and a project that has never had an
//!   `assets/` directory is not a project that lost one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bennu_wgsl::prelude::{scan_bindings, scan_symbols, WgslSymbolKind};

use crate::model::{MaterialDecl, ShaderUse, UniformStruct};
use crate::shader::BindingKind;

/// How bad a finding is. Mirrors the diagnostic severities the seam already carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One thing wrong between a material and its shader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderProblem {
    pub severity: Severity,
    pub message: String,
    /// The file to report it in — sometimes the `.rs`, sometimes the `.wgsl`.
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
    /// A stable id, so the same finding reads the same in the panel and in the editor.
    pub code: String,
}

/// One shader, and everything about the materials that name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderLink {
    /// The path as the Rust wrote it (`shaders/spiral_hover.wgsl`).
    pub asset_path: String,
    /// The file it resolved to, or `None` when nothing in the project has that path.
    pub file: Option<PathBuf>,
    /// Material type names that name this shader, with the stage each named it for.
    pub uses: Vec<ShaderUse>,
    pub problems: Vec<ShaderProblem>,
    /// What the shader itself declares in the material bind group, by index — so a catalog row
    /// can show the Rust side and the WGSL side of one binding together, which is the whole
    /// reason both files were read.
    pub bindings: Vec<ShaderBindingSite>,
    /// The structs the shader declares, with where their names and fields are.
    ///
    /// Offsets rather than text, and kept in the model rather than re-read: a go-to from the
    /// Rust side has to land on a line of a file that is not open, and the alternative is
    /// opening it from a navigation handler — which is the one thing an extension may not do.
    pub structs: Vec<ShaderStructSite>,
}

/// One `@group @binding` the shader declares, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderBindingSite {
    pub index: u32,
    /// `params: SpiralHoverParams` — the name and type, for a row that shows both sides.
    pub decl: String,
    /// Byte offsets of the variable's NAME.
    pub offset: usize,
    pub end: usize,
}

/// One `struct` the shader declares, and where its members are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderStructSite {
    pub name: String,
    pub offset: usize,
    pub end: usize,
    /// `(name, start, end)` per member, in declaration order.
    pub fields: Vec<(String, usize, usize)>,
}

impl ShaderLink {
    /// The shader's own declaration at `index`, as text (`params: SpiralHoverParams`).
    pub fn wgsl_binding(&self, index: u32) -> Option<&str> {
        self.bindings.iter().find(|b| b.index == index).map(|b| b.decl.as_str())
    }

    /// Where the shader declares the binding at `index`.
    pub fn binding_site(&self, index: u32) -> Option<&ShaderBindingSite> {
        self.bindings.iter().find(|b| b.index == index)
    }

    /// Where the shader declares `name`.
    pub fn struct_site(&self, name: &str) -> Option<&ShaderStructSite> {
        self.structs.iter().find(|s| s.name == name)
    }
}

/// A shader source the host handed over, keyed by the asset path it would be loaded as.
pub struct ShaderFile<'a> {
    pub path: &'a Path,
    pub text: &'a str,
    /// Every path this file answers to — see [`asset_paths_of`]. A shader can be both a file
    /// asset and an embedded one; which a material names is the material's business.
    pub asset_paths: Vec<String>,
}

/// The paths a `.wgsl` could be loaded by, in the two forms Bevy actually offers.
///
/// * **A file asset** — its path relative to the nearest `assets/` directory above it. What a
///   game ships.
/// * **An embedded asset** — `embedded://<crate>/<path under src/>`. What a *library* ships: a
///   crate that `embedded_asset!`s its shaders has them under `src/` and names them by that URL,
///   which is the whole reason a rendering crate can be depended on without also copying its
///   shaders into every consumer's `assets/`.
///
/// Both, rather than one or the other, because a workspace routinely has both — and because a
/// file under `src/shaders/` is unambiguously the second while a file under `assets/` is
/// unambiguously the first. A `.wgsl` under neither is not an asset at all: a test fixture, a
/// vendored copy, something a build script generates. It stays out rather than matching a
/// material by accident.
pub fn asset_paths_of(file: &Path) -> Vec<String> {
    let parts: Vec<String> =
        file.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let mut out = Vec::new();
    // The LAST `assets` component, not the first: a workspace at `~/assets/games/x/assets/…`
    // is somebody's directory name above and the real asset root below.
    if let Some(at) = parts.iter().rposition(|p| p == "assets") {
        let rest = &parts[at + 1..];
        if !rest.is_empty() {
            out.push(rest.join("/"));
        }
    }
    // `embedded://` names the crate, and Bevy derives the crate name from the *package* — whose
    // directory is conventionally the same with hyphens. `fulcrum-shading/src/shaders/stone.wgsl`
    // is therefore `embedded://fulcrum_shading/shaders/stone.wgsl`.
    if let Some(at) = parts.iter().rposition(|p| p == "src") {
        let rest = &parts[at + 1..];
        if at > 0 && !rest.is_empty() {
            let krate = parts[at - 1].replace('-', "_");
            out.push(format!("embedded://{krate}/{}", rest.join("/")));
        }
    }
    out
}

/// What the shader declares in the material bind group, with where each one is.
fn declared_bindings(text: &str) -> Vec<ShaderBindingSite> {
    scan_bindings(text)
        .into_iter()
        .filter(|b| b.in_material_group())
        .map(|b| ShaderBindingSite {
            index: b.index,
            decl: format!("{}: {}", b.name, b.ty),
            offset: b.start,
            end: b.start + b.name.len(),
        })
        .collect()
}

/// The structs the shader declares, with the span of each name and of every member.
fn declared_structs(text: &str) -> Vec<ShaderStructSite> {
    let symbols = scan_symbols(text);
    symbols
        .iter()
        .filter(|s| s.kind == WgslSymbolKind::Struct)
        .map(|s| ShaderStructSite {
            name: s.name.clone(),
            offset: s.start,
            end: s.end,
            fields: symbols
                .iter()
                .filter(|f| {
                    f.kind == WgslSymbolKind::Field && f.container.as_deref() == Some(&s.name)
                })
                .map(|f| (f.name.clone(), f.start, f.end))
                .collect(),
        })
        .collect()
}

/// Whether an asset path names an embedded asset rather than a file under `assets/`.
fn is_embedded(path: &str) -> bool {
    path.starts_with("embedded://")
}

/// The first of [`asset_paths_of`], for a caller that wants one name for the file.
pub fn asset_path_of(file: &Path) -> Option<String> {
    asset_paths_of(file).into_iter().next()
}

/// Join every material to the shader it names, and check the two against each other.
pub fn link(
    materials: &[MaterialDecl],
    layouts: &[UniformStruct],
    shaders: &[ShaderFile<'_>],
) -> Vec<ShaderLink> {
    let by_asset: HashMap<&str, &ShaderFile<'_>> = shaders
        .iter()
        .flat_map(|s| s.asset_paths.iter().map(move |p| (p.as_str(), s)))
        .collect();
    let layouts_by_name: HashMap<&str, &UniformStruct> =
        layouts.iter().map(|l| (l.name.as_str(), l)).collect();

    // Whether this project ships shaders in each of the two forms, decided once.
    //
    // "Nothing here has that path" is only a claim a project with such a root can make. An engine
    // crate embeds its shaders under `src/` and has no `assets/` at all, so every *file* path its
    // materials name belongs to the game that depends on it — and reporting those would put three
    // permanent errors on the crate this check was written against. Judged per form rather than
    // per project, because the same crate legitimately does one and not the other.
    let has_file_assets = shaders.iter().any(|s| s.asset_paths.iter().any(|p| !is_embedded(p)));
    let has_embedded = shaders.iter().any(|s| s.asset_paths.iter().any(|p| is_embedded(p)));

    let mut links: Vec<ShaderLink> = Vec::new();
    for material in materials {
        for used in &material.shaders {
            let slot = match links.iter().position(|l| l.asset_path == used.path) {
                Some(at) => at,
                None => {
                    let file = by_asset.get(used.path.as_str());
                    links.push(ShaderLink {
                        asset_path: used.path.clone(),
                        file: file.map(|f| f.path.to_path_buf()),
                        uses: Vec::new(),
                        problems: Vec::new(),
                        bindings: file.map(|f| declared_bindings(f.text)).unwrap_or_default(),
                        structs: file.map(|f| declared_structs(f.text)).unwrap_or_default(),
                    });
                    links.len() - 1
                }
            };
            links[slot].uses.push(ShaderUse {
                material: material.name.clone(),
                stage: used.stage.clone(),
                file: material.file.clone(),
                offset: used.offset,
                end: used.end,
                line: used.line,
            });
            let Some(shader) = by_asset.get(used.path.as_str()) else {
                // See the note above: a form this project does not ship in at all is a form its
                // consumer supplies.
                let claimable =
                    if is_embedded(&used.path) { has_embedded } else { has_file_assets };
                if claimable {
                    links[slot].problems.push(ShaderProblem {
                        severity: Severity::Error,
                        message: format!(
                            "no shader asset `{}` — nothing in this project's assets has that path",
                            used.path
                        ),
                        file: material.file.clone(),
                        start: used.offset,
                        end: used.end,
                        code: "bevy-shader-missing".to_string(),
                    });
                }
                continue;
            };
            let mut found = check(material, used, shader, &layouts_by_name);
            links[slot].problems.append(&mut found);
        }
    }
    // Every shader the project ships, including the ones nothing here names.
    //
    // Without this the panel is empty in exactly the project that has the shaders: a game whose
    // materials live in the engine crate it depends on — which is the layout in front of us.
    // The row carries no finding, because "no material in THIS project runs it" is not a defect:
    // the material is very often one project over. It answers the question the panel is for —
    // what shaders are here, and what runs each — rather than only half of it.
    for shader in shaders {
        let Some(path) = shader.asset_paths.first() else { continue };
        if shader.asset_paths.iter().any(|p| links.iter().any(|l| &l.asset_path == p)) {
            continue;
        }
        links.push(ShaderLink {
            asset_path: path.clone(),
            file: Some(shader.path.to_path_buf()),
            uses: Vec::new(),
            problems: Vec::new(),
            bindings: declared_bindings(shader.text),
            structs: declared_structs(shader.text),
        });
    }
    links.sort_by(|a, b| a.asset_path.cmp(&b.asset_path));
    links
}

/// Everything one (material, shader) pair disagrees about.
fn check(
    material: &MaterialDecl,
    used: &crate::model::ShaderRefDecl,
    shader: &ShaderFile<'_>,
    layouts: &HashMap<&str, &UniformStruct>,
) -> Vec<ShaderProblem> {
    let mut out = Vec::new();
    let symbols = scan_symbols(shader.text);
    let bindings = scan_bindings(shader.text);

    // 1. The entry point the stage needs.
    if let Some(attr) = entry_attribute(&used.stage) {
        let has_entry = symbols
            .iter()
            .any(|s| s.kind == WgslSymbolKind::EntryPoint && s.detail.contains(attr));
        if !has_entry {
            out.push(ShaderProblem {
                severity: Severity::Warning,
                message: format!(
                    "`{}` names this shader for the {} stage, but it declares no `@{attr}` entry point",
                    material.name, used.stage
                ),
                file: material.file.clone(),
                start: used.offset,
                end: used.end,
                code: "bevy-shader-entry-point".to_string(),
            });
        }
    }

    // 2. Every binding Rust promises has to exist in the shader's material group.
    for b in &material.bindings {
        if bindings.iter().any(|s| s.index == b.index && s.in_material_group()) {
            continue;
        }
        // A binding that IS in the file but in another group is a different mistake, and worth
        // saying so — "it is not there" would send the reader looking for the wrong thing.
        let elsewhere = bindings.iter().find(|s| s.index == b.index);
        let message = match elsewhere {
            Some(s) => format!(
                "`{}` binds {} at {}, but the shader declares `@binding({})` in `@group({})` — not the material's group",
                material.name, b.field, b.index, b.index, s.group
            ),
            None => format!(
                "`{}` binds {} at {}, and the shader declares no `@binding({})` in the material's group",
                material.name, b.field, b.index, b.index
            ),
        };
        out.push(ShaderProblem {
            severity: Severity::Warning,
            message,
            file: material.file.clone(),
            start: b.offset,
            end: b.offset + b.field.len(),
            code: "bevy-shader-binding-missing".to_string(),
        });
    }

    // 3. The layout of every uniform, field by field. The check this module exists for.
    for b in &material.bindings {
        if b.kind != BindingKind::Uniform {
            continue;
        }
        let Some(layout) = layouts.get(rust_type_name(&b.ty).as_str()) else { continue };
        let Some(wgsl_struct) = bindings
            .iter()
            .find(|s| s.index == b.index && s.in_material_group())
            .map(|s| wgsl_type_name(&s.ty))
        else {
            continue;
        };
        let wgsl_fields: Vec<(&str, &str)> = symbols
            .iter()
            .filter(|s| {
                s.kind == WgslSymbolKind::Field && s.container.as_deref() == Some(&wgsl_struct)
            })
            .map(|s| (s.name.as_str(), s.detail.as_str()))
            .collect();
        // Not in this file: it may arrive through an `#import`, and this side does not resolve
        // naga_oil's composition. Silence, not a claim.
        if wgsl_fields.is_empty() {
            continue;
        }
        out.extend(compare_layout(material, layout, &wgsl_struct, &wgsl_fields, shader.path));
    }
    out
}

/// Compare a Rust `ShaderType` against a WGSL struct, field by field.
fn compare_layout(
    material: &MaterialDecl,
    layout: &UniformStruct,
    wgsl_name: &str,
    wgsl_fields: &[(&str, &str)],
    shader_file: &Path,
) -> Vec<ShaderProblem> {
    let mut out = Vec::new();
    if layout.fields.len() != wgsl_fields.len() {
        out.push(ShaderProblem {
            severity: Severity::Warning,
            message: format!(
                "`{}` has {} field(s) and the shader's `{wgsl_name}` has {} — the uniform is read with a different layout than it is written",
                layout.name,
                layout.fields.len(),
                wgsl_fields.len()
            ),
            file: layout.file.clone(),
            start: layout.offset,
            end: layout.offset + layout.name.len(),
            code: "bevy-shader-layout-arity".to_string(),
        });
        return out;
    }
    for (rust, (wname, wty)) in layout.fields.iter().zip(wgsl_fields) {
        if &rust.name != wname {
            out.push(ShaderProblem {
                severity: Severity::Warning,
                message: format!(
                    "field {} of `{}` is `{}` here and `{wname}` in the shader's `{wgsl_name}` — a uniform is matched by position, so the two are the same bytes under different names",
                    rust.name, layout.name, rust.name
                ),
                file: layout.file.clone(),
                start: rust.offset,
                end: rust.offset + rust.name.len(),
                code: "bevy-shader-layout-name".to_string(),
            });
            continue;
        }
        // A type either side does not recognise ends the comparison for that field. Never a
        // guess: the cost of a wrong claim here is a user chasing a bug that is not there.
        let (Some(rust_ty), Some(wgsl_ty)) = (canonical_rust(&rust.ty), canonical_wgsl(wty)) else {
            continue;
        };
        if rust_ty != wgsl_ty {
            out.push(ShaderProblem {
                severity: Severity::Warning,
                message: format!(
                    "`{}.{}` is `{}` here and `{}` in the shader — {} bytes against {}",
                    layout.name, rust.name, rust.ty, wty, size_of(&rust_ty), size_of(&wgsl_ty)
                ),
                file: layout.file.clone(),
                start: rust.offset,
                end: rust.offset + rust.name.len(),
                code: "bevy-shader-layout-type".to_string(),
            });
        }
    }
    let _ = (material, shader_file);
    out
}

/// The `@…` a stage's entry point carries. `None` for a stage with no fixed attribute.
fn entry_attribute(stage: &str) -> Option<&'static str> {
    match stage {
        "fragment" | "prepass_fragment" | "deferred_fragment" => Some("fragment"),
        "vertex" | "prepass_vertex" => Some("vertex"),
        _ => None,
    }
}

/// A Rust type reduced to its last path segment, generics dropped.
fn rust_type_name(ty: &str) -> String {
    crate::items::last_segment(ty.split('<').next().unwrap_or(ty).trim())
}

/// A WGSL type reduced to its head — `SpiralHoverParams` out of `SpiralHoverParams`, and the
/// element type out of an array is deliberately NOT taken: a uniform of `array<T,N>` is a
/// different layout from one of `T`.
fn wgsl_type_name(ty: &str) -> String {
    ty.trim().to_string()
}

/// Both languages' scalar and vector types on one scale, so `Vec4` and `vec4<f32>` compare
/// equal and `Vec4` and `vec3<f32>` do not.
///
/// Anything not here answers `None`, which means "no opinion" everywhere it is used.
fn canonical_rust(ty: &str) -> Option<String> {
    let t = rust_type_name(ty);
    let out = match t.as_str() {
        "f32" => "f32",
        "u32" => "u32",
        "i32" => "i32",
        "Vec2" => "f32x2",
        "Vec3" | "Vec3A" => "f32x3",
        "Vec4" | "LinearRgba" | "Color" => "f32x4",
        "UVec2" => "u32x2",
        "UVec3" => "u32x3",
        "UVec4" => "u32x4",
        "IVec2" => "i32x2",
        "IVec3" => "i32x3",
        "IVec4" => "i32x4",
        "Mat2" => "mat2",
        "Mat3" | "Mat3A" => "mat3",
        "Mat4" => "mat4",
        _ => return None,
    };
    Some(out.to_string())
}

fn canonical_wgsl(ty: &str) -> Option<String> {
    let t = ty.trim();
    let out = match t {
        "f32" => "f32",
        "u32" => "u32",
        "i32" => "i32",
        "vec2<f32>" | "vec2f" => "f32x2",
        "vec3<f32>" | "vec3f" => "f32x3",
        "vec4<f32>" | "vec4f" => "f32x4",
        "vec2<u32>" | "vec2u" => "u32x2",
        "vec3<u32>" | "vec3u" => "u32x3",
        "vec4<u32>" | "vec4u" => "u32x4",
        "vec2<i32>" | "vec2i" => "i32x2",
        "vec3<i32>" | "vec3i" => "i32x3",
        "vec4<i32>" | "vec4i" => "i32x4",
        "mat2x2<f32>" | "mat2x2f" => "mat2",
        "mat3x3<f32>" | "mat3x3f" => "mat3",
        "mat4x4<f32>" | "mat4x4f" => "mat4",
        _ => return None,
    };
    Some(out.to_string())
}

/// The size a canonical type occupies, for the message. Not used for the comparison — two
/// different types of the same size are still two different types.
fn size_of(canonical: &str) -> usize {
    match canonical {
        "f32" | "u32" | "i32" => 4,
        "f32x2" | "u32x2" | "i32x2" => 8,
        "f32x3" | "u32x3" | "i32x3" => 12,
        "f32x4" | "u32x4" | "i32x4" => 16,
        "mat2" => 16,
        "mat3" => 48,
        "mat4" => 64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bennu_ext::prelude::ScannedFile;

    use super::*;
    use crate::build::build;

    /// The Rust half, in the shape a real Bevy material has: an `AsBindGroup` struct, a
    /// `ShaderType` layout, and an `impl Material` naming the shader.
    const MATERIAL: &str = r#"
use bevy::prelude::*;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SpiralHoverMaterial {
    #[uniform(0)]
    pub params: SpiralHoverParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SpiralHoverParams {
    pub sand_color: Vec4,
    pub dark_color: Vec4,
    pub spiral_speed: f32,
}

impl Material for SpiralHoverMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/spiral_hover.wgsl".into()
    }
}
"#;

    const SHADER: &str = r#"#import bevy_pbr::forward_io::VertexOutput

struct SpiralHoverParams {
    sand_color: vec4<f32>,
    dark_color: vec4<f32>,
    spiral_speed: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> params: SpiralHoverParams;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    return params.sand_color;
}
"#;

    fn model(rust: &str, shader: Option<&str>) -> crate::model::BevyModel {
        let sources =
            vec![ScannedFile { path: PathBuf::from("/p/src/mat.rs"), text: rust.to_string() }];
        let shaders: Vec<ScannedFile> = shader
            .map(|t| ScannedFile {
                path: PathBuf::from("/p/assets/shaders/spiral_hover.wgsl"),
                text: t.to_string(),
            })
            .into_iter()
            .collect();
        build(&sources, &shaders)
    }

    fn codes(m: &crate::model::BevyModel) -> Vec<&str> {
        m.shaders.iter().flat_map(|l| l.problems.iter()).map(|p| p.code.as_str()).collect()
    }

    #[test]
    fn a_material_that_agrees_with_its_shader_is_reported_as_nothing() {
        let m = model(MATERIAL, Some(SHADER));
        assert_eq!(m.shaders.len(), 1, "the shader is linked");
        assert_eq!(m.shaders[0].uses.len(), 1);
        assert_eq!(m.shaders[0].uses[0].material, "SpiralHoverMaterial");
        assert_eq!(m.shaders[0].uses[0].stage, "fragment");
        assert!(codes(&m).is_empty(), "{:?}", m.shaders[0].problems);
    }

    #[test]
    fn a_game_shader_is_named_relative_to_its_assets_directory() {
        assert_eq!(
            asset_paths_of(&PathBuf::from("/p/assets/shaders/x.wgsl")),
            vec!["shaders/x.wgsl".to_string()],
        );
    }

    #[test]
    fn a_library_shader_is_named_by_its_embedded_url() {
        // What `embedded_asset!` produces, and how an engine crate ships shaders at all: the
        // package directory's hyphens become the crate name's underscores.
        assert_eq!(
            asset_paths_of(&PathBuf::from(
                "/w/crates/feature/fulcrum-shading/src/shaders/stone.wgsl"
            )),
            vec!["embedded://fulcrum_shading/shaders/stone.wgsl".to_string()],
        );
    }

    #[test]
    fn a_shader_under_neither_is_not_an_asset() {
        // A fixture, a vendored copy, something a build script wrote. Matched against nothing
        // rather than against whichever material happens to name that file name.
        assert!(asset_paths_of(&PathBuf::from("/p/fixtures/x.wgsl")).is_empty());
    }

    #[test]
    fn a_shader_named_by_a_constant_is_still_named() {
        // An engine crate declares the path once at the top of the file and returns the
        // constant. Every material in `fulcrum-shading` is written this way, and a scan that
        // only looked inside the method found none of them.
        let rust = r#"
const STONE_SHADER: &str = "embedded://fulcrum_shading/shaders/stone.wgsl";

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct StoneExtension { #[uniform(100)] pub params: Vec4 }

impl MaterialExtension for StoneExtension {
    fn fragment_shader() -> ShaderRef {
        STONE_SHADER.into()
    }
}
"#;
        let sources = vec![ScannedFile {
            path: PathBuf::from("/w/crates/feature/fulcrum-shading/src/stone.rs"),
            text: rust.into(),
        }];
        let shaders = vec![ScannedFile {
            path: PathBuf::from("/w/crates/feature/fulcrum-shading/src/shaders/stone.wgsl"),
            text: "@fragment fn fragment() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }"
                .into(),
        }];
        let model = build(&sources, &shaders);
        assert_eq!(model.shaders.len(), 1);
        assert_eq!(model.shaders[0].asset_path, "embedded://fulcrum_shading/shaders/stone.wgsl");
        assert!(model.shaders[0].file.is_some(), "the embedded shader resolved");
        // The span points at the path inside the CONSTANT, so a go-to lands on the path.
        let used = &model.shaders[0].uses[0];
        assert_eq!(&rust[used.offset..used.end], "embedded://fulcrum_shading/shaders/stone.wgsl");
    }

    #[test]
    fn a_shader_that_is_not_there_is_an_error_on_the_path_that_names_it() {
        // The project HAS assets — one shader, at another path — so a path that resolves to
        // nothing is a real claim. See `library_tests` for the crate that ships none.
        let sources =
            vec![ScannedFile { path: PathBuf::from("/p/src/mat.rs"), text: MATERIAL.to_string() }];
        let shaders = vec![ScannedFile {
            path: PathBuf::from("/p/assets/shaders/other.wgsl"),
            text: "@fragment fn fragment() {}".to_string(),
        }];
        let m = build(&sources, &shaders);
        assert_eq!(codes(&m), vec!["bevy-shader-missing"]);
        let missing = m.shader("shaders/spiral_hover.wgsl").expect("the named shader has a row");
        let p = &missing.problems[0];
        assert_eq!(p.severity, Severity::Error);
        assert_eq!(&MATERIAL[p.start..p.end], "shaders/spiral_hover.wgsl");
    }

    #[test]
    fn a_field_the_shader_declares_with_another_type_is_the_bug_this_exists_for() {
        // `spiral_speed` is an `f32` in Rust and a `vec4<f32>` in the shader. It compiles, it
        // runs, and the uniform is read four floats wrong from there on — the afternoon this
        // check is meant to give back.
        let broken = SHADER.replace("spiral_speed: f32,", "spiral_speed: vec4<f32>,");
        let m = model(MATERIAL, Some(&broken));
        assert_eq!(codes(&m), vec!["bevy-shader-layout-type"]);
        let p = &m.shaders[0].problems[0];
        assert!(p.message.contains("f32"), "{}", p.message);
        assert!(p.message.contains("vec4<f32>"), "{}", p.message);
        // Reported where the Rust layout is written, on the field's own name.
        assert_eq!(p.file, PathBuf::from("/p/src/mat.rs"));
        assert_eq!(&MATERIAL[p.start..p.end], "spiral_speed");
    }

    #[test]
    fn a_field_out_of_order_is_named_rather_than_typed() {
        let swapped = SHADER.replace(
            "    sand_color: vec4<f32>,\n    dark_color: vec4<f32>,",
            "    dark_color: vec4<f32>,\n    sand_color: vec4<f32>,",
        );
        let m = model(MATERIAL, Some(&swapped));
        // Both positions disagree by name; the types still match, so this is the only thing said.
        assert_eq!(codes(&m), vec!["bevy-shader-layout-name", "bevy-shader-layout-name"]);
    }

    #[test]
    fn a_shader_with_fewer_fields_is_one_finding_not_a_cascade() {
        let short = SHADER.replace("    spiral_speed: f32,\n", "");
        let m = model(MATERIAL, Some(&short));
        assert_eq!(codes(&m), vec!["bevy-shader-layout-arity"]);
    }

    #[test]
    fn a_binding_the_shader_does_not_declare_is_reported_on_the_field() {
        let no_binding = SHADER.replace("@binding(0)", "@binding(3)");
        let m = model(MATERIAL, Some(&no_binding));
        assert!(codes(&m).contains(&"bevy-shader-binding-missing"), "{:?}", codes(&m));
    }

    #[test]
    fn a_binding_in_another_group_says_so_rather_than_saying_it_is_absent() {
        // The distinction matters: "it is not there" sends you looking for the wrong thing.
        let wrong_group = SHADER.replace("@group(#{MATERIAL_BIND_GROUP})", "@group(0)");
        let m = model(MATERIAL, Some(&wrong_group));
        let p = m.shaders[0]
            .problems
            .iter()
            .find(|p| p.code == "bevy-shader-binding-missing")
            .expect("the binding is not in the material's group");
        assert!(p.message.contains("not the material's group"), "{}", p.message);
    }

    #[test]
    fn a_missing_entry_point_is_a_warning_where_the_stage_was_named() {
        let no_entry = SHADER.replace("@fragment\n", "");
        let m = model(MATERIAL, Some(&no_entry));
        assert!(codes(&m).contains(&"bevy-shader-entry-point"), "{:?}", codes(&m));
    }

    #[test]
    fn a_struct_the_shader_imports_rather_than_declares_is_not_a_mismatch() {
        // naga_oil composition is not resolved on this side, so a struct that is not in the file
        // is unknown rather than absent — and silence is the only honest answer.
        let imported = SHADER.replace(
            "struct SpiralHoverParams {\n    sand_color: vec4<f32>,\n    dark_color: vec4<f32>,\n    spiral_speed: f32,\n};\n",
            "#import fulcrum::spiral::SpiralHoverParams\n",
        );
        let m = model(MATERIAL, Some(&imported));
        assert!(
            !codes(&m).iter().any(|c| c.starts_with("bevy-shader-layout")),
            "{:?}",
            m.shaders[0].problems
        );
    }

    #[test]
    fn a_type_neither_side_recognises_ends_the_comparison_rather_than_guessing() {
        let rust = MATERIAL.replace("pub spiral_speed: f32,", "pub spiral_speed: MyOwnScalar,");
        let m = model(&rust, Some(SHADER));
        assert!(
            !codes(&m).iter().any(|c| *c == "bevy-shader-layout-type"),
            "an unknown Rust type must produce no claim: {:?}",
            m.shaders[0].problems
        );
    }

    #[test]
    fn two_materials_naming_one_shader_are_one_row_with_two_uses() {
        let rust = format!(
            "{MATERIAL}\n#[derive(Asset, TypePath, AsBindGroup, Clone)]\npub struct GhostMaterial {{ #[uniform(0)] pub params: SpiralHoverParams }}\nimpl Material2d for GhostMaterial {{ fn fragment_shader() -> ShaderRef {{ \"shaders/spiral_hover.wgsl\".into() }} }}\n"
        );
        let m = model(&rust, Some(SHADER));
        assert_eq!(m.shaders.len(), 1);
        let names: Vec<&str> = m.shaders[0].uses.iter().map(|u| u.material.as_str()).collect();
        assert_eq!(names, vec!["GhostMaterial", "SpiralHoverMaterial"]);
    }
}

#[cfg(test)]
mod library_tests {
    use std::path::PathBuf;

    use bennu_ext::prelude::ScannedFile;

    use crate::build::build;

    /// An engine crate: it declares the material, and the game that depends on it ships the
    /// shader. Real, and the layout of the project this check was written against.
    #[test]
    fn a_crate_that_ships_no_assets_is_told_nothing_about_a_shader_it_does_not_have() {
        let rust = r#"
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SpiralHoverMaterial { #[uniform(0)] pub params: SpiralHoverParams }
impl Material for SpiralHoverMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/spiral_hover.wgsl".into() }
}
"#;
        let sources =
            vec![ScannedFile { path: PathBuf::from("/engine/src/spiral.rs"), text: rust.into() }];
        let model = build(&sources, &[]);
        // The link is still there — the row says which shader the material runs, which is worth
        // knowing. What it does not carry is a complaint.
        assert_eq!(model.shaders.len(), 1);
        assert_eq!(model.shaders[0].asset_path, "shaders/spiral_hover.wgsl");
        assert!(model.shaders[0].file.is_none());
        assert!(
            model.shaders[0].problems.is_empty(),
            "a library's shader is its consumer's to ship: {:?}",
            model.shaders[0].problems
        );
    }

    /// The same material in a project that DOES ship shaders: now the path is a real claim.
    #[test]
    fn a_game_with_assets_is_told_when_a_path_resolves_to_nothing() {
        let rust = r#"
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct GhostMaterial { #[uniform(0)] pub params: GhostParams }
impl Material for GhostMaterial {
    fn fragment_shader() -> ShaderRef { "shaders/typo.wgsl".into() }
}
"#;
        let sources =
            vec![ScannedFile { path: PathBuf::from("/game/src/ghost.rs"), text: rust.into() }];
        let shaders = vec![ScannedFile {
            path: PathBuf::from("/game/assets/shaders/real.wgsl"),
            text: "@fragment fn fragment() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }"
                .into(),
        }];
        let model = build(&sources, &shaders);
        let codes: Vec<&str> =
            model.shaders.iter().flat_map(|l| l.problems.iter()).map(|p| p.code.as_str()).collect();
        assert_eq!(codes, vec!["bevy-shader-missing"]);
    }
}
