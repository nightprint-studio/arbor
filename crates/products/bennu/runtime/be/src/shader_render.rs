//! `bennu_shader_render` — compile a WGSL material, render it, and hand back the picture.
//!
//! ## Why a tool and not the panel
//!
//! The shader-preview panel already renders a material, and for a person that is the right
//! surface: it is live, it animates, and it is beside the file. None of that reaches an
//! assistant. Asked to tune a shader it can read the source and reason about it, but it cannot
//! *look* at the result — so a change that compiles and is wrong looks exactly like one that is
//! right. This closes that: parameters in, image out, in one call.
//!
//! Together with [`crate::shader_uniform`] it is a loop rather than a shot in the dark: ask what
//! the material declares, write values at those names, look, adjust.
//!
//! ## Where the pixels come from
//!
//! The `bevy-runtime` package's own renderer, run headless. Not a wgpu program written here,
//! because the shaders this has to render are **Bevy's**: `#import bevy_pbr::forward_io::…`
//! resolves against the engine's shader library, with the mesh and view bind groups laid out
//! the way Bevy lays them out. Reproducing that outside Bevy is a project that would drift from
//! the engine at its first release — and the picture would stop being the picture the panel
//! shows, which is the only thing that makes it worth looking at.
//!
//! So the renderer ships with the package that already owns the viewport, and this locates and
//! runs it. A machine without that package installed gets a sentence saying so rather than a
//! feature that silently is not there.
//!
//! ## What it is drawn on
//!
//! Whatever the panel can draw it on. The built-in shapes are here, and so is every mesh an
//! installed `mesh-source` package offers, addressed `<package>/<shape>` — see
//! [`crate::shader_mesh`] for why that matters more than it sounds: most of what goes wrong
//! with a material goes wrong in the coupling between the shader and the geometry, and a
//! sphere is the one shape that cannot show it.
//!
//! ## What "it compiled" means here
//!
//! wgpu does not refuse a bad shader — it logs and renders nothing. So a run whose log carries
//! shader errors is reported as an **error with those lines verbatim**: they name the line and
//! the type, and rewording them would lose the only part that helps. A clean run answers with
//! the image.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use base64::Engine as _;
use bennu_core::prelude::BennuState;
use bennu_wgsl::prelude::{
    material_bind_group, preview_plan_with, PreviewCaps, SlotFamily, TEXTURE_2D_ARRAY_SLOTS,
    TEXTURE_2D_SLOTS,
};
use serde::{Deserialize, Serialize};

/// The renderer's file name inside the `bevy-runtime` package.
#[cfg(windows)]
const RENDERER_EXE: &str = "arbor-shader-render.exe";
#[cfg(not(windows))]
const RENDERER_EXE: &str = "arbor-shader-render";

/// Longest a single render may take. A shader that makes the driver spin should end the call,
/// not the session.
const RENDER_TIMEOUT_SECS: u64 = 90;

// ── Arguments ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ShaderRenderArgs {
    /// The `.wgsl` file's absolute path. Used to read the shader when `source` is absent.
    #[serde(default)]
    pub path: Option<String>,
    /// The shader's text, for a buffer that is not on disk. Preferred over `path` when both
    /// are given — an editor asking about what it is showing means what it is showing.
    #[serde(default)]
    pub source: Option<String>,
    /// Values for the material's own parameters, **by the name the shader gave them**. A
    /// scalar is a number; a `vec3`/`vec4` is an array; a matrix is an array in column-major
    /// order. Read `bennu_shader_uniform` first to learn the names.
    ///
    /// A field you leave out is written as **zero**, which for most materials means black — so
    /// an empty call is a valid way to see the geometry and a poor way to see the material.
    #[serde(default)]
    pub params: Option<HashMap<String, serde_json::Value>>,
    /// The packed uniform block, if you would rather write the bytes yourself. Floats in
    /// buffer order. Ignored when `params` is given.
    ///
    /// For a material that extends `StandardMaterial` the layout is one **128-float slot per
    /// binding** from 100 up, not one `vec4` — a slot is 512 bytes because a shader is free to
    /// bind a struct or an `array<vec4<f32>, 32>` there. Prefer `params`: it writes each value
    /// at the offset the shader reads it from, which is the part that is easy to get wrong and
    /// impossible to see in the picture.
    #[serde(default)]
    pub data: Option<Vec<f32>>,
    /// What to draw the material on.
    ///
    /// Built in: `sphere` (default) · `cube` · `plane` · `torus` · `capsule` · `cylinder`. A
    /// sphere shows a lighting term from every angle; a plane is right for a shader that
    /// writes colour and not shape.
    ///
    /// **Or a mesh from an installed package**, addressed `<package>/<shape>` —
    /// `fulcrum/hex_tile`, `primitives/sphere`. That is usually the one you want: a shader is
    /// written against geometry, and a material whose grass only grows on upward faces or
    /// whose sides are meant to stay bare cannot be judged on a sphere at all. A name that is
    /// neither is an error listing what is installed, not a silent sphere.
    #[serde(default)]
    pub mesh: Option<String>,
    /// Values for the mesh's own parameters, by the names its JSON Schema declares — read
    /// them from the shape's `params-schema` in the package's `catalogue()`.
    ///
    /// Only for a mesh from a package: a built-in shape has no schema, and passing parameters
    /// with one is reported rather than ignored, because a caller who tunes a subdivision
    /// count and sees no change has been told the shape reacts to it.
    #[serde(default)]
    pub mesh_params: Option<serde_json::Value>,
    /// The instant to render, in seconds, for a material that animates. Default 0. The clock
    /// is pinned, so the same value renders the same image every time.
    #[serde(default)]
    pub time: Option<f32>,
    /// Square edge in pixels. Default 512, maximum 4096.
    #[serde(default)]
    pub size: Option<u32>,
    /// Camera distance from the origin. Default 2.6.
    #[serde(default)]
    pub distance: Option<f32>,
    /// Camera pitch in radians, positive looking down. Default 0.3.
    #[serde(default)]
    pub pitch: Option<f32>,
    /// Draw a chequerboard behind the material instead of a flat fill. Default true, and worth
    /// keeping: a material that computes its own alpha looks identical over any single colour.
    #[serde(default)]
    pub checker: Option<bool>,
    /// Blend mode for the material: `blend` (default) · `opaque` · `premultiplied` · `add` ·
    /// `multiply`.
    #[serde(default)]
    pub alpha: Option<String>,
    /// What to put in a texture the material samples — `white` · `black` · `grey` ·
    /// `normal` · `checker` · `noise` · `uv`.
    ///
    /// Keyed by the shader's own variable name (`top_normal`) **or by what the texture is**
    /// (`normal`, `diffuse`, `pbr`, `ao`), which reaches every face at once. The variable's
    /// name wins where both match.
    ///
    /// A render has no assets: the atlas the game feeds this material lives in a project the
    /// renderer cannot reach. So each texture is filled with an image it generates, chosen by
    /// default from what it is — a `normal` gets a flat normal, a `height` mid grey, a
    /// `diffuse` a chequer. Name one here to override that.
    ///
    /// `uv` is the diagnostic one: it paints the coordinates themselves, which is how you see
    /// that a material is sampling the wrong rectangle of an atlas rather than that its
    /// colours are wrong.
    #[serde(default)]
    pub textures: Option<HashMap<String, String>>,
    /// Force rendering as an **extension of `StandardMaterial`** — the Bevy convention where a
    /// material declares `vec4`s at bindings 100 and up and is lit by the scene.
    ///
    /// Leave it out. Which one a shader is follows from its binding indices and is read from
    /// the source; overriding it wrongly does not change a colour, it produces a pipeline whose
    /// layout does not match the shader and the render fails validation.
    #[serde(default)]
    pub extension: Option<bool>,
}

/// A PNG, handed back inline. The shape `output = image` expects.
#[derive(Serialize, schemars::JsonSchema)]
pub struct InlineImage {
    /// Always `image/png`.
    pub mime_type: String,
    /// Base64, with no data-URI prefix.
    pub data: String,
}

// ── Finding the renderer ────────────────────────────────────────────────────────

/// Where the `bevy-runtime` package keeps its headless renderer.
///
/// The same roots the plugin host scans, in the same order, so "installed" means the same
/// thing here as it does in the Plugin Manager. The environment variable is the escape hatch
/// for a checkout that builds the renderer somewhere else.
fn renderer_path() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("ARBOR_SHADER_RENDERER") {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "ARBOR_SHADER_RENDERER points at '{}', which is not a file",
            p.display()
        ));
    }

    let roots = [
        arbor_plugin_core::prelude::plugin_dir(),
        arbor_core::prelude::marketplace_plugins_dir(),
    ];
    for root in roots {
        let candidate = root.join("bevy-runtime").join("bin").join(RENDERER_EXE);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err("the shader renderer is not built. It ships with the `bevy-runtime` package: install \
         that package, then run its `build-render.sh` once to produce \
         `bevy-runtime/bin/arbor-shader-render`."
        .to_string())
}

// ── Packing ─────────────────────────────────────────────────────────────────────

/// Write named values into the uniform block, at the offsets the shader's own struct implies.
///
/// The offsets come from `bennu-wgsl`, so WGSL's alignment is honoured: the padding a `vec4`
/// forces after three scalars is real padding here, and a matrix is written column by column at
/// its own stride — which is the rule that makes a `mat3x3<f32>` 48 bytes and not 36. Writing
/// values consecutively instead is quiet and wrong: everything lands one field along and the
/// material renders something plausible that is not what was asked for.
///
/// A name that is not a field is reported rather than ignored. A caller that misspells one and
/// gets a picture back has been told the shader does not react to it, which is worse than
/// being told the name is wrong.
/// How many `vec4` slots the runtime's extension material offers. Must match `EXT_SLOTS` there.
const EXT_SLOTS: usize = 8;

/// Floats one slot holds — 128, which is the runtime's 512-byte `ExtSlot`.
///
/// Not four. A material extension binds one uniform per binding from 100 up, and what it binds
/// there is not always a `vec4`: `tile.wgsl` puts a 45-member struct at 116 and an
/// `array<vec4<f32>, 32>` is an ordinary thing to want. Writing an extension's blocks four
/// floats apart put every member past the first four into the next slot, where the shader
/// never reads it — and the values that did land read as zero.
const SLOT_FLOATS: usize = 128;

/// What the renderer needs: the floats, and which material has to be built for them.
#[derive(Debug)]
struct Packed {
    floats: Vec<f32>,
    /// True when the material owns its whole bind group and the floats are one buffer; false
    /// when it extends `StandardMaterial` and they are `EXT_SLOTS` × `vec4`, in slot order.
    owns_group: bool,
}

fn pack_named(
    source: &str,
    values: &HashMap<String, serde_json::Value>,
) -> Result<Packed, String> {
    let group = material_bind_group(source)
        .ok_or("this shader binds no parameter block that can be laid out")?;
    let owns_group = group.owns_group();
    if group.blocks.is_empty() {
        return Err("this shader binds no parameter block that can be laid out".to_string());
    }

    // One buffer for a material that owns its group; `EXT_SLOTS` × `vec4` for an extension,
    // where each binding from 100 up is its own slot. Not the same arithmetic wearing two
    // names: writing an extension's five uniforms into one contiguous buffer puts every one of
    // them at an offset the shader does not read.
    let width = if owns_group {
        (group.blocks[0].size as usize).div_ceil(4)
    } else {
        EXT_SLOTS * SLOT_FLOATS
    };
    let mut floats = vec![0.0f32; width];
    let mut known: Vec<String> = Vec::new();

    for block in &group.blocks {
        // Where this block's first float lands in `floats`.
        let base = if owns_group {
            0usize
        } else {
            let slot = (block.binding as usize).saturating_sub(100);
            if slot >= EXT_SLOTS {
                return Err(format!(
                    "this material binds at {}, past the {} slots the preview offers — it \
                     cannot be rendered here",
                    block.binding, EXT_SLOTS
                ));
            }
            slot * SLOT_FLOATS
        };

        for field in &block.fields {
            known.push(field.name.clone());
            let Some(v) = values.get(&field.name) else { continue };

            let lanes: Vec<f32> = match v {
                serde_json::Value::Number(n) => vec![n.as_f64().unwrap_or(0.0) as f32],
                serde_json::Value::Array(a) => {
                    a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()
                }
                other => {
                    return Err(format!(
                        "'{}' wants a number or an array of numbers, got {other}",
                        field.name
                    ))
                }
            };

            if field.columns > 1 {
                // Column-major, each column at its own stride.
                for c in 0..field.columns as usize {
                    let at = base + (field.offset as usize + c * field.column_stride as usize) / 4;
                    for r in 0..field.rows as usize {
                        if let Some(f) = lanes.get(c * field.rows as usize + r) {
                            if let Some(slot) = floats.get_mut(at + r) {
                                *slot = *f;
                            }
                        }
                    }
                }
            } else {
                let at = base + field.offset as usize / 4;
                for (i, f) in lanes.iter().take(field.rows as usize).enumerate() {
                    if let Some(slot) = floats.get_mut(at + i) {
                        *slot = *f;
                    }
                }
            }
        }
    }

    let unknown: Vec<String> = values
        .keys()
        .filter(|k| !known.iter().any(|n| n == *k))
        .map(|k| format!("'{k}'"))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "{} is not a parameter of this material. It declares: {}",
            unknown.join(", "),
            known.join(", ")
        ));
    }

    Ok(Packed { floats, owns_group })
}

// ── The log ─────────────────────────────────────────────────────────────────────

/// The lines of the renderer's log that a caller has to see.
///
/// wgpu answers a bad shader by logging and drawing nothing, so this is the whole of "did it
/// compile". Kept verbatim: a naga error names the line and the type, and every rewording of
/// one loses the part that helps.
fn shader_errors(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| {
            let low = l.to_ascii_lowercase();
            low.contains("error") && !low.contains("error_handler")
        })
        .map(|l| l.trim().to_string())
        .collect()
}

// ── The handler ─────────────────────────────────────────────────────────────────

/// Render a WGSL material and return the image.
///
/// Use it to **see** what a shader does: after changing it, after setting parameters, or to
/// compare two parameter sets — the clock is pinned, so the only thing that differs between two
/// calls is what you changed. It also answers "does this compile": a shader whose pipeline
/// fails comes back as an error carrying the compiler's own lines, with the line numbers.
///
/// Read `bennu_shader_uniform` first to learn the parameter names; values are given by name and
/// anything omitted is written as zero.
#[arbor_rpc::handler(mcp(
    name = "bennu_shader_render",
    title = "Render a WGSL material",
    safety = read,
    output = image,
))]
fn bennu_shader_render(ctx: &BennuState, args: ShaderRenderArgs) -> Result<InlineImage, String> {
    let renderer = renderer_path()?;

    // The geometry first, because it is the argument most likely to be wrong and the one whose
    // failure used to be invisible: an unknown name fell through onto a sphere, so a render
    // asked about a tile answered about a ball and reported success.
    //
    // Blocking host calls, and safe here: the serve loop dispatches each request on its own
    // worker thread, which is the same reason this may wait ninety seconds on a child.
    let mesh_name = args.mesh.as_deref().unwrap_or("sphere");
    let geometry = crate::shader_mesh::build(ctx, mesh_name, args.mesh_params.as_ref())?;

    // The source, and a file for it. The renderer takes a path because a shader is a file
    // everywhere else in this system; a buffer that is not on disk gets a temporary one.
    let source = match (&args.source, &args.path) {
        (Some(text), _) => text.clone(),
        (None, Some(p)) => std::fs::read_to_string(p)
            .map_err(|e| format!("cannot read {p}: {e}"))?,
        (None, None) => return Err("give either `path` or `source`".to_string()),
    };

    // The shader is RENUMBERED onto the renderer's fixed layout before anything else looks at
    // it. That is what lets a material with textures and samplers be rendered at all: a
    // previewer's bind-group layout is decided when IT is compiled, so it cannot grow a
    // sampler at 101 because this shader wants one there — but the shader can be moved onto
    // the slots that do exist. See `bennu_wgsl::preview_layout`.
    //
    // Everything downstream reads the rewritten copy: the temporary file the renderer opens,
    // and `pack_named`, whose slot arithmetic is "binding minus the base" and is only true of
    // a shader already on the slots. Field names and offsets are untouched, so the names a
    // caller learnt from `bennu_shader_uniform` still mean what they meant.
    // The FULL set of slots: this is the headless renderer, which is native and has room for
    // every texture a material declares. The panel's viewport asks for far fewer, because a
    // fragment stage on WebGL2 gets 16 texture units in total — which is why a material can
    // render here and be refused there, and why that is worth saying rather than hiding.
    let plan = preview_plan_with(&source, PreviewCaps::native());
    if let Some(p) = &plan {
        if !p.rejected.is_empty() {
            let names: Vec<String> = p
                .rejected
                .iter()
                .map(|r| format!("{} (@{}) — {}", r.name, r.binding, r.reason))
                .collect();
            return Err(format!(
                "this material binds resources the renderer cannot supply: {}. Everything else \
                 about it renders; these do not, and a picture with them missing would be a \
                 different material rather than an incomplete one.",
                names.join("; ")
            ));
        }
    }
    let source = plan.as_ref().map(|p| p.source.clone()).unwrap_or(source);

    // One role per texture slot, in the order the runtime fills them. A name the caller gave
    // wins over the guess from the variable's name; a name that is not a texture is reported,
    // because a caller who misspells one and gets a picture back has been told the material
    // ignores it.
    // The list is POSITIONAL and covers all three families in the order the runtime fills
    // them — the 2D slots, then the array slots, then the cubes. A cube's role therefore sits
    // at index 14 even when the shader has one texture, and a list built by walking `placed`
    // in binding order would put it at index 1 and paint the wrong slot.
    let mut roles: Vec<String> = Vec::new();
    if let Some(p) = &plan {
        let asked = args.textures.clone().unwrap_or_default();
        let mut named: Vec<String> = Vec::new();
        for (family, offset) in [
            (SlotFamily::Texture2d, 0u32),
            (SlotFamily::Texture2dArray, TEXTURE_2D_SLOTS),
            (SlotFamily::TextureCube, TEXTURE_2D_SLOTS + TEXTURE_2D_ARRAY_SLOTS),
        ] {
            for t in p.family(family) {
                let at = (offset + t.slot) as usize;
                if named.len() <= at {
                    named.resize(at + 1, String::new());
                }
                // By the variable's name, or by what it IS — `{"normal": "uv"}` reaches
                // `top_normal` and `side_normal` at once, which is the whole point of a key.
                // The variable's own name wins, so naming one of a pair still singles it out.
                let choice = asked
                    .get(&t.name)
                    .or_else(|| asked.get(&t.key))
                    .cloned()
                    .unwrap_or_else(|| t.image.clone());
                // First writer wins: several textures share a slot, and the one that owns it
                // is the one listed first.
                if named[at].is_empty() {
                    named[at] = choice;
                }
            }
        }
        roles = named;

        let unknown: Vec<String> = asked
            .keys()
            .filter(|k| {
                !p.placed
                    .iter()
                    .any(|t| !t.key.is_empty() && (t.name == **k || t.key == **k))
            })
            .map(|k| format!("'{k}'"))
            .collect();
        if !unknown.is_empty() {
            let known: Vec<String> = p
                .placed
                .iter()
                .filter(|t| !t.key.is_empty())
                .map(|t| format!("{} ({})", t.name, t.key))
                .collect();
            return Err(format!(
                "{} is neither a texture this material samples nor a kind it samples. It has: {}",
                unknown.join(", "),
                known.join(", ")
            ));
        }
    }

    // Always a temporary file, even when the caller gave a path: what the renderer has to
    // compile is the renumbered copy, and writing that back over somebody's shader would be a
    // preview editing the thing it is previewing.
    let shader_path =
        std::env::temp_dir().join(format!("arbor-shader-{}.wgsl", std::process::id()));
    std::fs::write(&shader_path, &source)
        .map_err(|e| format!("cannot write the shader: {e}"))?;

    let out = std::env::temp_dir().join(format!("arbor-shader-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&out);

    // Parameters, either already packed or written by name into the blocks the shader declares.
    //
    // Which MATERIAL to build comes from the same reading. It is not the caller's choice by
    // default: a shader that extends `StandardMaterial` rendered as a raw material — or the
    // reverse — is not a wrong colour, it is a pipeline whose layout does not match the shader,
    // and wgpu refuses it outright. `extension` stays as an override for the case where the
    // caller knows better, which is rare enough to be worth having and wrong enough to default.
    let (data, packed_owns_group) = match (&args.params, &args.data) {
        (Some(named), _) => {
            let p = pack_named(&source, named)?;
            (p.floats, Some(p.owns_group))
        }
        (None, Some(raw)) => (raw.clone(), None),
        (None, None) => {
            // Nothing given: still read the shader, so the right material is built and an
            // untouched material renders instead of failing validation.
            let owns = material_bind_group(&source).map(|g| g.owns_group());
            (Vec::new(), owns)
        }
    };
    let extension = args.extension.unwrap_or_else(|| !packed_owns_group.unwrap_or(true));

    let size = args.size.unwrap_or(512).clamp(1, 4096);
    let mut cmd = Command::new(&renderer);
    cmd.arg("--shader").arg(&shader_path)
        .arg("--out").arg(&out)
        .arg("--size").arg(format!("{size}x{size}"))
        .arg("--mesh").arg(mesh_name)
        .arg("--time").arg(format!("{}", args.time.unwrap_or(0.0)))
        .arg("--distance").arg(format!("{}", args.distance.unwrap_or(2.6)))
        .arg("--pitch").arg(format!("{}", args.pitch.unwrap_or(0.3)))
        .arg("--alpha").arg(args.alpha.as_deref().unwrap_or("blend"))
        .arg("--checker").arg(if args.checker.unwrap_or(true) { "on" } else { "off" });
    if extension {
        cmd.arg("--extension");
    }
    // Vertices go through a FILE. A mesh worth previewing is tens of thousands of floats,
    // which is past what any platform's command line will carry.
    let mesh_path = std::env::temp_dir().join(format!("arbor-shader-mesh-{}.json", std::process::id()));
    if let Some(data) = &geometry {
        let text = serde_json::to_string(data)
            .map_err(|e| format!("cannot encode the mesh: {e}"))?;
        std::fs::write(&mesh_path, text)
            .map_err(|e| format!("cannot write the mesh: {e}"))?;
        cmd.arg("--mesh-file").arg(&mesh_path);
    }
    if !data.is_empty() {
        let list: Vec<String> = data.iter().map(|f| format!("{f}")).collect();
        cmd.arg("--data").arg(list.join(","));
    }
    if !roles.is_empty() {
        cmd.arg("--textures").arg(roles.join(","));
    }
    // Lo stadio vertex, quando lo shader ne porta uno. Letto dal piano e non offerto come
    // argomento: sbagliarlo non cambia un colore — un `@vertex` ignorato in silenzio da una
    // parte, una pipeline che non compila dall'altra.
    if plan.as_ref().is_some_and(|p| p.vertex_entry) {
        cmd.arg("--vertex");
    }

    let result = run_with_timeout(cmd, RENDER_TIMEOUT_SECS);
    let _ = std::fs::remove_file(&shader_path);
    if geometry.is_some() {
        let _ = std::fs::remove_file(&mesh_path);
    }
    let (status_ok, stderr) = result?;

    let errors = shader_errors(&stderr);
    if !errors.is_empty() {
        let _ = std::fs::remove_file(&out);
        return Err(format!("the material did not compile:\n{}", errors.join("\n")));
    }
    if !status_ok {
        return Err(format!(
            "the renderer failed:\n{}",
            tail(&stderr, 20)
        ));
    }

    let bytes = std::fs::read(&out)
        .map_err(|e| format!("the renderer wrote no image: {e}\n{}", tail(&stderr, 20)))?;
    let _ = std::fs::remove_file(&out);

    Ok(InlineImage {
        mime_type: "image/png".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// The last `n` lines, for an error message that should be readable rather than complete.
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Run the renderer, giving up after `secs`.
///
/// A wait with a deadline, not a plain `output()`: a shader can put a driver into a loop, and
/// a backend thread blocked forever on a child is a product that stops answering — with no
/// error, which is the shape of failure that costs the most to diagnose.
fn run_with_timeout(mut cmd: Command, secs: u64) -> Result<(bool, String), String> {
    use std::io::Read as _;
    use std::process::Stdio;

    // stdin null and not inherited: fd 0 is the protocol pipe, and a child that makes it
    // non-blocking makes it non-blocking for the backend too — see `child.rs::run_streamed`.
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("cannot start the renderer: {e}"))?;

    // The pipe is drained on this thread after the wait, which is safe here because the
    // renderer's log is small and bounded — a few dozen lines. A chattier child would need the
    // reader running alongside the wait to avoid filling the pipe buffer.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut err = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut err);
                }
                return Ok((status.success(), err));
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(format!("the render did not finish within {secs}s"));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(format!("waiting for the renderer failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = r#"
struct Params {
    tint: vec4<f32>,
    strength: f32,
    warp: mat3x3<f32>,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> params: Params;
"#;

    fn values(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn a_vector_lands_at_its_own_offset() {
        let packed = pack_named(SHADER, &values(&[("tint", serde_json::json!([1.0, 2.0, 3.0, 4.0]))]))
            .expect("packs").floats;
        assert_eq!(&packed[0..4], &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_scalar_after_a_vec4_is_not_at_index_four_by_accident() {
        // It is — a vec4 is 16 bytes — but the point is that the offset comes from the
        // description rather than from counting the fields, which is what breaks on a vec3.
        let packed = pack_named(SHADER, &values(&[("strength", serde_json::json!(0.5))])).expect("packs").floats;
        assert_eq!(packed[4], 0.5);
    }

    #[test]
    fn a_matrix_is_written_column_by_column_at_its_stride() {
        // A `mat3x3<f32>` is three columns of three floats, each column padded to 16 bytes —
        // so the second column starts four floats along, not three.
        let m = serde_json::json!([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
        let packed = pack_named(SHADER, &values(&[("warp", m)])).expect("packs").floats;
        let base = 8; // vec4 (4) + f32 (1) + padding to the matrix's 16-byte alignment
        assert_eq!(packed[base], 1.0);
        assert_eq!(packed[base + 4], 4.0);
        assert_eq!(packed[base + 8], 7.0);
    }

    #[test]
    fn a_name_the_material_does_not_have_is_refused() {
        let err = pack_named(SHADER, &values(&[("tnit", serde_json::json!(1.0))]))
            .expect_err("a misspelling must not pass silently");
        assert!(err.contains("'tnit'"), "{err}");
        assert!(err.contains("tint"), "the message should list what it does declare: {err}");
    }

    #[test]
    fn omitted_fields_are_zero() {
        let packed = pack_named(SHADER, &values(&[("strength", serde_json::json!(1.0))])).expect("packs").floats;
        assert_eq!(&packed[0..4], &[0.0, 0.0, 0.0, 0.0]);
    }

    const EXTENSION: &str = r#"
struct Big {
    a: vec4<f32>,
    b: vec4<f32>,
};
@group(3) @binding(100)
var<uniform> first: Big;
@group(3) @binding(101)
var<uniform> second: vec4<f32>;
"#;

    #[test]
    fn an_extension_block_bigger_than_a_vec4_does_not_spill_into_the_next_binding() {
        let packed = pack_named(
            EXTENSION,
            &values(&[
                ("b", serde_json::json!([9.0, 9.0, 9.0, 9.0])),
                ("second", serde_json::json!([5.0, 0.0, 0.0, 0.0])),
            ]),
        )
        .expect("packs");
        assert!(!packed.owns_group);
        // `b` is the second `vec4` of the block at binding 100, so it belongs at float 4 of
        // slot 0 — and slot 1 must still be the binding at 101, untouched by it.
        assert_eq!(&packed.floats[4..8], &[9.0, 9.0, 9.0, 9.0]);
        assert_eq!(packed.floats[SLOT_FLOATS], 5.0);
    }

    #[test]
    fn shader_errors_keep_the_compiler_words() {
        let log = "INFO bevy_render: adapter\nERROR bevy_render: Shader validation error: \
                   at line 12: unknown identifier 'foo'\nINFO done";
        let found = shader_errors(log);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("line 12"), "{:?}", found);
    }
}
