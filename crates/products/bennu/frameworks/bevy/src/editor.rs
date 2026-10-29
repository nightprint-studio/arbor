//! What the open buffer gets: the gutter marks beside its declarations, and the warning on a pair
//! of systems nothing orders.
//!
//! ## Anchored in the buffer, answered from the model
//!
//! Both of these carry an **offset into the file in front of you**, and the model was built from
//! what was on disk at the last reindex — so an offset taken from it lands a line late as soon as
//! anything above it is typed. Everything anchored here is therefore re-read from `source` on each
//! call (one linear scan of one file, on the editor's debounce), and the model is consulted only
//! for what lives *elsewhere*: which systems touch this type, which system this one contends with.
//! A target in another file may be stale by a line; a squiggle under the caret may not.
//!
//! It is the same division `bennu-jpa` makes for the same reason.

use bennu_ext::prelude::{ExtGutterMark, ExtTarget};
use std::path::Path;

use bennu_proto::prelude::{severity, Diagnostic};

use bennu_complete::prelude::line_number;

use crate::conflict::warnable;
use crate::items::{scan_file, RawFn};
use crate::mask::mask;
use bennu_wgsl::prelude::{
    scan_bindings as scan_wgsl_bindings, scan_symbols as scan_wgsl, WgslSymbolKind,
};

use crate::model::{access_keys, BevyModel, Role, SystemDecl};

/// A mark beside every ECS declaration in `source`, pointing at the systems that touch it.
pub fn gutter(model: &BevyModel, source: &str) -> Vec<ExtGutterMark> {
    scan_file(&mask(source))
        .types
        .into_iter()
        .map(|t| {
            let touching = model.touching(&access_keys(&t.name, &t.roles));
            let writers = touching.iter().filter(|(_, a)| a.kind.writes()).count();
            // A marker's users are its filters, and a mark that ignored them would be a dot beside
            // a component the whole project queries on saying nobody wants it.
            let filtering = model.filtering(&t.name);
            let role = t.roles.first().copied().unwrap_or(Role::Component);
            ExtGutterMark {
                line: line_number(source, t.offset),
                kind: role.gutter_kind().to_string(),
                tooltip: format!(
                    "{} — {}",
                    role.label(),
                    touch_summary(touching.len(), writers, filtering.len())
                ),
                // Read/written first, then the systems that only filter on it: the same order the
                // tooltip counts them in, and `filter` rather than a made-up read/write.
                targets: touching
                    .iter()
                    .map(|(s, a)| target(s, a.kind.label()))
                    .chain(filtering.iter().map(|(s, _)| target(s, "filter")))
                    .collect(),
            }
        })
        .collect()
}

/// One jump target: the system, and in one word what it does with the declaration.
fn target(s: &SystemDecl, how: &str) -> ExtTarget {
    ExtTarget {
        file: s.file.to_string_lossy().replace('\\', "/"),
        offset: s.offset,
        label: s.name.clone(),
        detail: format!("{how} · {}", s.schedules.first().map_or("unregistered", String::as_str)),
    }
}

fn touch_summary(total: usize, writers: usize, filters: usize) -> String {
    let mut parts = Vec::new();
    match (total, writers) {
        (0, _) => {}
        (n, 0) => parts.push(format!("read by {n}")),
        (n, w) if w == n => parts.push(format!("written by {w}")),
        (n, w) => parts.push(format!("{} read, {w} written", n - w)),
    }
    if filters > 0 {
        parts.push(format!("filtered on by {filters}"));
    }
    if parts.is_empty() {
        return "no system in this project touches it".to_string();
    }
    parts.join(" · ")
}

/// One note per system in this buffer that contends with others and is ordered against none of
/// them.
///
/// **Only the unordered pairs.** A conflict is not a defect — two systems that want the same data
/// and say in which order is exactly how an ECS is written. What is worth mentioning is the pair
/// where the order was never stated: it is decided by the schedule, it can change when an unrelated
/// system is added, and it is invisible until the day it is wrong. The full list, ordered pairs
/// included, stays in the Access conflicts panel.
///
/// **One note per system, at `info`.** Read/write contention without a declared order is the normal
/// state of nearly every Bevy project, and often intended; a `warning` per peer put a wall of
/// squiggles under working code. So the peers are folded into one line on the system's name, at the
/// level that informs without alarming.
///
/// **Matched to the buffer by file and position, not by name.** `model.systems` for this file are
/// paired with the buffer's functions of the same name by nearest offset: the model's offsets may
/// be a few lines stale, but a same-named function in another module, or an `impl` method called
/// like a system, sits much further away than that — and one that is not a system in the model
/// claims nothing.
pub fn diagnostics(model: &BevyModel, path: &Path, source: &str) -> Vec<Diagnostic> {
    let scanned = scan_file(&mask(source));
    let mut out = Vec::new();
    for (index, system) in model.systems.iter().enumerate() {
        if !same_file(&system.file, path) {
            continue;
        }
        let Some(f) = nearest_fn(&scanned.fns, system) else { continue };
        let findings = unordered_findings(model, index);
        if findings.peers.is_empty() {
            continue;
        }
        out.push(Diagnostic {
            message: findings.message(),
            severity: severity::INFO.to_string(),
            code: "bevy.unordered-conflict".to_string(),
            start: f.offset,
            end: f.offset + f.name.len(),
        });
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Whether two paths name one file, tolerating the separator the host happened to use.
fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/");
    norm(a) == norm(b)
}

/// The buffer's function that `system` is — the same-named one closest to where the model saw it.
fn nearest_fn<'a>(fns: &'a [RawFn], system: &SystemDecl) -> Option<&'a RawFn> {
    let distance = |f: &RawFn| f.offset.abs_diff(system.offset);
    fns.iter().filter(|f| f.name == system.name).min_by_key(|f| distance(f))
}

/// Everything the unordered, warnable conflicts say about one system.
struct Findings {
    name: String,
    /// Distinct peers, in the order first met.
    peers: Vec<String>,
    /// Distinct contended types, in the order first met.
    targets: Vec<String>,
    schedules: Vec<String>,
}

/// How many names a note spells out before it says `…`.
const NAMED_PEERS: usize = 3;

impl Findings {
    fn message(&self) -> String {
        let shown = self.peers.iter().take(NAMED_PEERS).cloned().collect::<Vec<_>>().join(", ");
        let more = if self.peers.len() > NAMED_PEERS { "…" } else { "" };
        let n = self.peers.len();
        format!(
            "`{}` contends with {n} unordered system{} ({shown}{more}) over {} in {} — nothing in \
             this project orders them, so they run in whichever order the schedule picks",
            self.name,
            if n == 1 { "" } else { "s" },
            self.targets.join(", "),
            self.schedules.join(", "),
        )
    }
}

fn unordered_findings(model: &BevyModel, index: usize) -> Findings {
    let mut f = Findings {
        name: model.systems[index].name.clone(),
        peers: Vec::new(),
        targets: Vec::new(),
        schedules: Vec::new(),
    };
    let push = |list: &mut Vec<String>, item: &str| {
        if !list.iter().any(|x| x == item) {
            list.push(item.to_string());
        }
    };
    for c in &model.conflicts {
        if !warnable(c, &model.systems) {
            continue;
        }
        let peer = if c.a == index {
            c.b
        } else if c.b == index {
            c.a
        } else {
            continue;
        };
        push(&mut f.peers, &model.systems[peer].name);
        push(&mut f.schedules, &c.schedule);
        for r in &c.reasons {
            push(&mut f.targets, &r.target);
        }
    }
    f
}

// ── materials and their shaders ──────────────────────────────────────────────────

/// The findings that belong under the caret in `path`, re-anchored in the buffer in front of you.
///
/// Re-anchored, not replayed. The model's offsets were taken at the last reindex; the buffer may
/// have moved every one of them, and a squiggle a line off the thing it is about is worse than
/// none. So each problem is matched back to the buffer **by name** — the shader path it names,
/// the field it is about — and dropped when the buffer no longer has it, which is exactly the
/// case where the user has already fixed it.
///
/// Both sides of the seam land here: a missing asset is reported in the `.rs` that names it, a
/// layout mismatch in the file that declares the layout. The extension asks about whichever file
/// is open and gets its half.
pub fn shader_diagnostics(model: &BevyModel, path: &std::path::Path, source: &str) -> Vec<Diagnostic> {
    let problems = model.shader_problems_in(path);
    if problems.is_empty() {
        return Vec::new();
    }
    let scan = crate::shader::scan(&mask(source), source);
    problems
        .into_iter()
        .filter_map(|p| {
            let (start, end) = anchor(&scan, p, source)?;
            Some(Diagnostic {
                message: p.message.clone(),
                severity: p.severity.as_str().to_string(),
                code: p.code.clone(),
                start,
                end,
            })
        })
        .collect()
}

/// Where a problem points *now*.
///
/// The old span is the fallback rather than the answer: it is right until the file is edited, and
/// it is checked against the source before it is trusted — a span that no longer contains the
/// text it was taken from is a span that has moved.
fn anchor(
    scan: &crate::shader::ShaderScan,
    problem: &crate::shader_link::ShaderProblem,
    source: &str,
) -> Option<(usize, usize)> {
    // A path: find the literal again. Two stages may name the same shader, and either is a fine
    // place to say it is missing.
    if let Some(r) = scan.refs.iter().find(|r| problem.message.contains(&r.path)) {
        return Some((r.offset, r.end));
    }
    // A binding or a layout field: find the name again.
    let name = source.get(problem.start..problem.end)?;
    if !name.is_empty() {
        for m in &scan.materials {
            if let Some(b) = m.bindings.iter().find(|b| b.field == name) {
                return Some((b.offset, b.offset + b.field.len()));
            }
        }
        for st in &scan.structs {
            if st.name == name {
                return Some((st.offset, st.offset + st.name.len()));
            }
            if let Some(f) = st.fields.iter().find(|f| f.name == name) {
                return Some((f.offset, f.offset + f.name.len()));
            }
        }
    }
    // Unchanged text at the recorded span: the file has not moved under it.
    (source.get(problem.start..problem.end) == Some(name)).then_some((problem.start, problem.end))
}

/// A mark beside every material in `source`, pointing at the shaders it runs.
///
/// The affordance the shader catalog exists to make unnecessary, exactly as the ECS gutter is for
/// the components one: the answer to "which shader is this" belongs beside the declaration.
pub fn shader_gutter(model: &BevyModel, source: &str) -> Vec<ExtGutterMark> {
    let scan = crate::shader::scan(&mask(source), source);
    scan.materials
        .iter()
        .filter_map(|m| {
            let known = model.materials.iter().find(|d| d.name == m.name)?;
            if known.shaders.is_empty() {
                return None;
            }
            let targets: Vec<ExtTarget> = known
                .shaders
                .iter()
                .filter_map(|s| {
                    let link = model.shader(&s.path)?;
                    let file = link.file.as_ref()?;
                    Some(ExtTarget {
                        file: file.to_string_lossy().replace('\\', "/"),
                        offset: 0,
                        label: s.path.clone(),
                        detail: format!("{} stage", s.stage),
                    })
                })
                .collect();
            Some(ExtGutterMark {
                line: line_number(source, m.offset),
                kind: "shader".to_string(),
                tooltip: match targets.len() {
                    0 => format!(
                        "Material — names {} shader(s), none of which resolved to an asset",
                        known.shaders.len()
                    ),
                    _ => format!(
                        "Material — {}",
                        known
                            .shaders
                            .iter()
                            .map(|s| format!("{}: {}", s.stage, s.path))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ),
                },
                targets,
            })
        })
        .collect()
}

/// Go-to across the seam, in whichever direction the caret is pointing.
///
/// The seam has three joins, and the caret says which one it is on. Each works both ways, because
/// a declaration split over two files has no primary half — you are as likely to be reading the
/// shader and wanting the Rust as the other way round.
///
/// | caret is on | answers with |
/// |---|---|
/// | the shader path in `fragment_shader()` | the `.wgsl` |
/// | a `#[uniform(0)]` field | the shader's `@binding(0)` |
/// | a `ShaderType` struct or one of its fields | the shader's `struct`, or that member |
/// | a shader `struct` or member | the Rust layout, or that field |
/// | a shader binding variable | the Rust field that supplies it |
/// | anywhere else in a `.wgsl` | the materials that run it |
///
/// The last row is the fallback rather than the answer: a shader has no single declaration to
/// jump to, and "who runs this" is what somebody reading one usually wants when the caret is not
/// on anything in particular.
pub fn shader_navigate(
    model: &BevyModel,
    path: &std::path::Path,
    source: &str,
    offset: usize,
) -> Vec<ExtTarget> {
    match path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase().as_str() {
        "wgsl" => from_shader(model, path, source, offset),
        "rs" => from_rust(model, source, offset),
        _ => Vec::new(),
    }
}

/// A jump target at a Rust or WGSL site.
fn site_target(file: &std::path::Path, offset: usize, label: &str, detail: String) -> ExtTarget {
    ExtTarget {
        file: file.to_string_lossy().replace('\\', "/"),
        offset,
        label: label.to_string(),
        detail,
    }
}

/// From a `.wgsl`, in three steps of decreasing precision.
fn from_shader(
    model: &BevyModel,
    path: &std::path::Path,
    source: &str,
    offset: usize,
) -> Vec<ExtTarget> {
    let Some(asset) = crate::shader_link::asset_path_of(path) else { return Vec::new() };
    let Some(link) = model.shader(&asset) else { return Vec::new() };

    // 1. A member of a struct, or the struct's own name. The Rust half is the `ShaderType`
    //    layout of the same name — the two descriptions of one block of bytes.
    if let Some((owner, member)) = struct_at(source, offset) {
        if let Some(layout) = model.uniforms.iter().find(|u| u.name == owner) {
            let field = member.and_then(|m| layout.fields.iter().find(|f| f.name == m));
            return vec![match field {
                Some(f) => site_target(
                    &layout.file,
                    f.offset,
                    &format!("{}.{}", layout.name, f.name),
                    format!("{} · the Rust side of this member", f.ty),
                ),
                None => site_target(
                    &layout.file,
                    layout.offset,
                    &layout.name,
                    "#[derive(ShaderType)] — the layout this struct has to match".to_string(),
                ),
            }];
        }
    }

    // 2. A binding variable. Its Rust half is the field carrying `#[uniform(n)]`.
    if let Some(index) = binding_index_at(source, offset) {
        let mut out = Vec::new();
        for used in &link.uses {
            let Some(material) = model.materials.iter().find(|m| m.name == used.material) else {
                continue;
            };
            if let Some(b) = material.bindings.iter().find(|b| b.index == index) {
                out.push(site_target(
                    &material.file,
                    b.offset,
                    &format!("{}.{}", material.name, b.field),
                    format!("#[{}({})]", b.kind.label(), b.index),
                ));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }

    // 3. Anywhere else: who runs this shader.
    link.uses
        .iter()
        .map(|u| {
            site_target(&u.file, u.offset, &u.material, format!("{} stage · {asset}", u.stage))
        })
        .collect()
}

/// From a `.rs`, by what the caret is on.
fn from_rust(model: &BevyModel, source: &str, offset: usize) -> Vec<ExtTarget> {
    let scan = crate::shader::scan(&mask(source), source);

    // 1. The shader path itself.
    if let Some(r) = scan.refs.iter().find(|r| offset >= r.offset && offset <= r.end) {
        let Some(link) = model.shader(&r.path) else { return Vec::new() };
        let Some(file) = &link.file else { return Vec::new() };
        return vec![site_target(
            file,
            0,
            r.path.rsplit('/').next().unwrap_or(&r.path),
            format!("{} shader of {}", r.stage, r.type_name),
        )];
    }

    // 2. A `#[uniform(n)]` field → the shader's `@binding(n)`, in every shader the material runs.
    for material in &scan.materials {
        let Some(b) = material.bindings.iter().find(|b| word_at(source, offset, b.offset, &b.field))
        else {
            continue;
        };
        let Some(decl) = model.materials.iter().find(|m| m.name == material.name) else { continue };
        let mut out = Vec::new();
        for used in &decl.shaders {
            let Some(link) = model.shader(&used.path) else { continue };
            let Some(file) = &link.file else { continue };
            if let Some(site) = link.binding_site(b.index) {
                out.push(site_target(
                    file,
                    site.offset,
                    &site.decl,
                    format!("@binding({}) in {}", b.index, used.path),
                ));
            }
        }
        if !out.is_empty() {
            return out;
        }
    }

    // 3. A `ShaderType` struct or one of its fields → the shader's struct, or that member.
    //
    //    Every shader in the project is searched rather than only the ones a material using this
    //    layout runs: a layout is reached through a `#[uniform]` and following that chain would
    //    answer nothing for a struct nested inside another, which is a real and ordinary shape.
    for st in &scan.structs {
        let on_name = word_at(source, offset, st.offset, &st.name);
        let member = st.fields.iter().find(|f| word_at(source, offset, f.offset, &f.name));
        if !on_name && member.is_none() {
            continue;
        }
        let mut out = Vec::new();
        for link in &model.shaders {
            let (Some(file), Some(site)) = (&link.file, link.struct_site(&st.name)) else {
                continue;
            };
            let field = member.and_then(|m| site.fields.iter().find(|(n, _, _)| n == &m.name));
            out.push(match field {
                Some((n, at, _)) => site_target(
                    file,
                    *at,
                    &format!("{}.{n}", st.name),
                    format!("the shader's member · {}", link.asset_path),
                ),
                None => site_target(
                    file,
                    site.offset,
                    &st.name,
                    format!("the shader's struct · {}", link.asset_path),
                ),
            });
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// Whether `offset` falls inside the word of length `name` starting at `at`.
fn word_at(source: &str, offset: usize, at: usize, name: &str) -> bool {
    offset >= at && offset <= at + name.len() && source.get(at..at + name.len()) == Some(name)
}

/// The struct the caret is in, and the member it is on — `(owner, Some(member))` on a member,
/// `(owner, None)` on the struct's own name.
fn struct_at(source: &str, offset: usize) -> Option<(String, Option<String>)> {
    for s in scan_wgsl(source) {
        if offset < s.start || offset > s.end {
            continue;
        }
        return match s.kind {
            WgslSymbolKind::Struct => Some((s.name, None)),
            WgslSymbolKind::Field => s.container.clone().map(|c| (c, Some(s.name))),
            _ => None,
        };
    }
    None
}

/// The binding index of the variable the caret is on.
fn binding_index_at(source: &str, offset: usize) -> Option<u32> {
    scan_wgsl_bindings(source)
        .into_iter()
        .find(|b| offset >= b.start && offset <= b.end)
        .map(|b| b.index)
}

#[cfg(test)]
mod nav_tests {
    use std::path::{Path, PathBuf};

    use bennu_ext::prelude::ScannedFile;

    use crate::build::build;
    use crate::model::BevyModel;

    use super::shader_navigate;

    const RUST: &str = r#"
use bevy::prelude::*;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SpiralHoverMaterial {
    #[uniform(0)]
    pub params: SpiralHoverParams,
}

#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SpiralHoverParams {
    pub sand_color: Vec4,
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
    spiral_speed: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> params: SpiralHoverParams;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    return params.sand_color;
}
"#;

    const RS_PATH: &str = "/p/src/mat.rs";
    const WGSL_PATH: &str = "/p/assets/shaders/spiral_hover.wgsl";

    fn model() -> BevyModel {
        build(
            &[ScannedFile { path: PathBuf::from(RS_PATH), text: RUST.to_string() }],
            &[ScannedFile { path: PathBuf::from(WGSL_PATH), text: SHADER.to_string() }],
        )
    }

    /// The caret in the middle of the `nth` occurrence of `needle`.
    fn caret(text: &str, needle: &str, nth: usize) -> usize {
        let mut from = 0;
        for _ in 0..nth {
            from = text[from..].find(needle).expect("occurrence") + from + needle.len();
        }
        text[from..].find(needle).expect("occurrence") + from + needle.len() / 2
    }

    fn from_wgsl(at: usize) -> Vec<super::ExtTarget> {
        shader_navigate(&model(), Path::new(WGSL_PATH), SHADER, at)
    }
    fn from_rs(at: usize) -> Vec<super::ExtTarget> {
        shader_navigate(&model(), Path::new(RS_PATH), RUST, at)
    }

    // ── shader → Rust ────────────────────────────────────────────────────────

    #[test]
    fn a_shader_struct_goes_to_the_rust_layout() {
        let t = from_wgsl(caret(SHADER, "SpiralHoverParams", 0));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, RS_PATH);
        assert_eq!(t[0].label, "SpiralHoverParams");
        assert_eq!(&RUST[t[0].offset..t[0].offset + 17], "SpiralHoverParams");
        assert!(t[0].detail.contains("ShaderType"), "{}", t[0].detail);
    }

    #[test]
    fn a_shader_member_goes_to_the_rust_field_it_matches() {
        let t = from_wgsl(caret(SHADER, "spiral_speed", 0));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "SpiralHoverParams.spiral_speed");
        assert_eq!(&RUST[t[0].offset..t[0].offset + 12], "spiral_speed");
        // The detail names the Rust type, which is the thing you crossed the seam to check.
        assert!(t[0].detail.contains("f32"), "{}", t[0].detail);
    }

    #[test]
    fn a_shader_binding_goes_to_the_field_that_supplies_it() {
        // Inside the NAME of the `var<uniform> params` declaration, not a use of `params` in
        // the body — `caret` lands in the middle of its needle, which here would be inside the
        // address space.
        let at = SHADER.find("var<uniform> params").unwrap() + "var<uniform> ".len() + 2;
        let t = from_wgsl(at);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "SpiralHoverMaterial.params");
        assert_eq!(t[0].detail, "#[uniform(0)]");
        assert_eq!(&RUST[t[0].offset..t[0].offset + 6], "params");
    }

    #[test]
    fn anywhere_else_in_a_shader_answers_with_the_materials() {
        let t = from_wgsl(caret(SHADER, "fn fragment", 0));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "SpiralHoverMaterial");
    }

    // ── Rust → shader ────────────────────────────────────────────────────────

    #[test]
    fn a_rust_layout_goes_to_the_shader_struct() {
        // The DECLARATION of `SpiralHoverParams`, not its use as the uniform's type.
        let t = from_rs(caret(RUST, "pub struct SpiralHoverParams", 0) + "pub struct ".len());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, WGSL_PATH);
        assert_eq!(&SHADER[t[0].offset..t[0].offset + 17], "SpiralHoverParams");
    }

    #[test]
    fn a_rust_field_goes_to_the_shader_member() {
        let t = from_rs(caret(RUST, "pub sand_color", 0) + "pub ".len());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].label, "SpiralHoverParams.sand_color");
        assert_eq!(&SHADER[t[0].offset..t[0].offset + 10], "sand_color");
    }

    #[test]
    fn a_uniform_field_goes_to_the_shaders_binding() {
        let t = from_rs(caret(RUST, "pub params", 0) + "pub ".len());
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, WGSL_PATH);
        assert!(t[0].detail.contains("@binding(0)"), "{}", t[0].detail);
        assert_eq!(&SHADER[t[0].offset..t[0].offset + 6], "params");
    }

    #[test]
    fn the_shader_path_still_opens_the_shader() {
        let t = from_rs(caret(RUST, "shaders/spiral_hover.wgsl", 0));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].file, WGSL_PATH);
        assert_eq!(t[0].label, "spiral_hover.wgsl");
    }

    #[test]
    fn a_caret_on_nothing_in_particular_answers_nothing() {
        // In a `.rs` there is no fallback: the Java-stack resolvers below this one have their
        // own answers, and inventing a jump here would take the caret somewhere unrelated.
        assert!(from_rs(caret(RUST, "use bevy", 0)).is_empty());
    }
}

#[cfg(test)]
mod unordered_tests {
    use std::path::{Path, PathBuf};

    use bennu_ext::prelude::ScannedFile;

    use crate::build::build;
    use crate::model::BevyModel;

    use super::diagnostics;

    const PATH: &str = "/p/src/m.rs";

    fn model_of(src: &str) -> BevyModel {
        build(&[ScannedFile { path: PathBuf::from(PATH), text: src.to_string() }], &[])
    }

    /// Where each flagged system's name starts, in buffer order.
    fn flagged(src: &str) -> Vec<usize> {
        flagged_with_messages(src).into_iter().map(|(at, _)| at).collect()
    }

    fn flagged_with_messages(src: &str) -> Vec<(usize, String)> {
        diagnostics(&model_of(src), Path::new(PATH), src)
            .into_iter()
            .map(|d| (d.start, d.message))
            .collect()
    }

    fn at(src: &str, needle: &str) -> usize {
        src.find(needle).unwrap_or_else(|| panic!("`{needle}` not in source"))
    }

    // -- must warn ---------------------------------------------------------------

    #[test]
    fn warns_on_a_component_write_against_a_read_with_no_order() {
        let src = "fn a(mut q: Query<&mut Transform>) {}\nfn b(q: Query<&Transform>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert_eq!(flagged(src), vec![at(src, "a(mut"), at(src, "b(q")]);
    }

    #[test]
    fn warns_on_two_unordered_writers_of_one_resource() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert_eq!(flagged(src).len(), 2);
    }

    #[test]
    fn a_fan_in_leaves_the_two_sources_unordered() {
        // a and c both run before b; nothing says which of the two goes first.
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b() {}\nfn c(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a.before(b), c.before(b), b)); }";
        assert_eq!(flagged(src), vec![at(src, "a(mut"), at(src, "c(mut")]);
    }

    #[test]
    fn one_note_per_system_folds_all_its_peers() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(mut s: ResMut<Score>) {}\n\
                   fn c(mut s: ResMut<Score>) {}\nfn d(mut s: ResMut<Score>) {}\n\
                   fn e(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b, c, d, e)); }";
        let found = flagged_with_messages(src);
        assert_eq!(found.len(), 5, "five systems, five notes - not one per pair");
        let m = &found[0].1;
        assert!(m.contains("4 unordered systems (b, c, d…)"), "{m}");
        assert!(m.contains("Score") && m.contains("Update"), "{m}");
    }

    #[test]
    fn the_note_is_informational_not_a_warning() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(s: Res<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        let d = diagnostics(&model_of(src), Path::new(PATH), src);
        assert!(!d.is_empty() && d.iter().all(|d| d.severity == "info"));
    }

    // -- must not warn -----------------------------------------------------------

    #[test]
    fn with_and_without_the_same_marker_do_not_contend() {
        let src = "fn a(mut q: Query<&mut Health, With<Enemy>>) {}\n\
                   fn b(mut q: Query<&mut Health, Without<Enemy>>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert!(flagged(src).is_empty());
    }

    #[test]
    fn a_borrowed_component_implies_with_so_a_without_on_it_is_disjoint() {
        // Literal shape: nothing is shared at all.
        let src = "fn a(mut q: Query<&mut A>) {}\nfn b(mut q: Query<&mut B, Without<A>>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert!(flagged(src).is_empty());
        // Shared `B`: the first query only matches entities that have `A`, the second only ones
        // that do not - Bevy accepts that, so it is not a contention.
        let src = "fn a(mut q: Query<(&mut A, &mut B)>) {}\n\
                   fn b(mut q: Query<&mut B, Without<A>>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert!(flagged(src).is_empty());
        // An optional borrow requires nothing, so the same pair does contend.
        let src = "fn a(mut q: Query<(Option<&mut A>, &mut B)>) {}\n\
                   fn b(mut q: Query<&mut B, Without<A>>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert_eq!(flagged(src).len(), 2);
    }

    #[test]
    fn a_change_filter_read_carries_its_querys_filters() {
        let src = "fn a(mut q: Query<&mut Health, With<Enemy>>) {}\n\
                   fn b(q: Query<&Name, (Changed<Health>, Without<Enemy>)>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert!(flagged(src).is_empty());
    }

    #[test]
    fn transitive_before_edges_order_the_ends() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b() {}\nfn c(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a.before(b), b.before(c), c)); }";
        assert!(flagged(src).is_empty());
    }

    #[test]
    fn a_chain_and_an_after_both_order() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b() {}\nfn c(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b, c).chain()); }";
        assert!(flagged(src).is_empty());
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b() {}\nfn c(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) {\n app.add_systems(Update, (b, c.after(b)));\n\
                   app.add_systems(Update, a.before(b));\n}";
        assert!(flagged(src).is_empty());
    }

    #[test]
    fn run_conditions_silence_the_pair() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) {\n\
                   app.add_systems(Update, a.run_if(in_state(S::A)));\n\
                   app.add_systems(Update, b.run_if(in_state(S::B)));\n}";
        assert!(flagged(src).is_empty());
        // A condition on the whole tuple counts too.
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b).run_if(playing)); }";
        assert!(flagged(src).is_empty());
    }

    #[test]
    fn a_message_writer_and_reader_are_a_handoff_not_a_race() {
        let src = "fn w(mut e: EventWriter<Hit>) {}\nfn r(mut e: EventReader<Hit>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (w, r)); }";
        assert!(flagged(src).is_empty());
        // Two writers of the same buffer still interleave their messages in an unstated order.
        let src = "fn w(mut e: EventWriter<Hit>) {}\nfn v(mut e: EventWriter<Hit>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (w, v)); }";
        assert_eq!(flagged(src).len(), 2);
    }

    #[test]
    fn findings_attach_to_the_right_function_among_same_named_ones() {
        let src = "\
mod first { pub fn tick(mut s: ResMut<Score>) {} }
mod second { pub fn tick(q: Query<&Health>) {} }
struct Thing;
impl Thing { fn bump(&self, n: u32) {} }
fn bump(mut s: ResMut<Score>) {}
#[cfg(test)]
mod tests {
    fn t1(mut s: ResMut<Score>) {}
    fn t2(s: Res<Score>) {}
    #[test]
    fn it_runs() { App::new().add_systems(Update, (t1, t2)); }
}
fn p(app: &mut App) { app.add_systems(Update, (tick, bump)); }";
        let model = model_of(src);
        assert!(
            model.systems.iter().all(|s| s.name != "t1" && s.name != "t2"),
            "test-only systems are not part of the schedule"
        );
        let found = flagged(src);
        // `first::tick` and the free `bump` - not `second::tick`, not the `impl` method.
        assert_eq!(found, vec![at(src, "tick(mut"), at(src, "bump(mut")], "{found:?}");
    }

    #[test]
    fn a_buffer_for_another_file_gets_nothing() {
        let src = "fn a(mut s: ResMut<Score>) {}\nfn b(mut s: ResMut<Score>) {}\n\
                   fn p(app: &mut App) { app.add_systems(Update, (a, b)); }";
        assert!(diagnostics(&model_of(src), Path::new("/p/src/other.rs"), src).is_empty());
    }
}
