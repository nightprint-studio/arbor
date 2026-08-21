//! `arbor.shader` — the WGSL namespace Bennu publishes to the plugins it hosts.
//!
//! This is Bennu graduating from **host-pure**. Until now it published only the namespaces
//! `register_lua_api` hardcodes; a plugin running here could read files and draw panels, but
//! could not ask Bennu anything Bennu knows.
//!
//! ## Why a plugin has to be able to ask
//!
//! A shader preview needs the parameters the shader declares — names, types, byte offsets.
//! There were three places that could have read them, and only one that should:
//!
//! · the **plugin**, in Lua, by pattern-matching the source. Wrong twice over before it was
//!   even finished: WGSL block comments nest, and `@group(0)` is the view's bind group, not
//!   the material's.
//! · the **Bevy runtime**, in the viewport. The one place with the least to read with — no
//!   project, no import library, nothing to resolve `bevy_pbr` against — and it would be a
//!   third parser for one language.
//! · **Bennu**, which already reads WGSL for highlighting, hover and for checking a material's
//!   Rust half against its shader half.
//!
//! So the plugin asks, and the answer comes from the same code the editor's own features
//! depend on — which is what keeps it right.
//!
//! ## The shape this sets
//!
//! `arbor.shader.uniform{ source = … }` is the first of these. What makes it a good precedent
//! is that it is a **question about a document**, not a hook into Bennu's UI: pure, cheap,
//! and answerable without a project being open. A namespace that started by exposing "select
//! this range in the editor" would have made every later addition an argument about how much
//! of the product a plugin gets to drive.

use std::sync::Arc;

use arbor_plugin_core::prelude::{ApiCtx, LuaNamespaceInstaller};
use bennu_wgsl::prelude::{
    material_bind_group, preview_plan_with, PreviewCaps, SlotFamily, TEXTURE_2D_ARRAY_SLOTS,
    TEXTURE_2D_SLOTS,
};
use mlua::{Lua, Table};

/// `source = "…"` or `path = "…"`, whichever the caller passed.
///
/// Shared by every function in the namespace, so a plugin does not have to remember which of
/// them accepts a path — and so the message naming the caller stays the caller's.
fn read_source(cfg: &Table, who: &str) -> mlua::Result<String> {
    let source: Option<String> = cfg.get("source").ok();
    let path: Option<String> = cfg.get("path").ok();
    match (source, path) {
        (Some(s), _) if !s.is_empty() => Ok(s),
        (_, Some(p)) => std::fs::read_to_string(&p)
            .map_err(|e| mlua::Error::RuntimeError(format!("{p}: {e}"))),
        _ => Err(mlua::Error::RuntimeError(format!("{who}: pass `source` or `path`"))),
    }
}

/// Publishes `arbor.shader`.
pub struct ShaderNs;

impl LuaNamespaceInstaller for ShaderNs {
    fn install(
        &self,
        _ctx: &ApiCtx,
        lua: &Lua,
        arbor: &Table,
    ) -> arbor_plugin_core::prelude::PluginCoreResult<()> {
        let err = |e: mlua::Error| {
            arbor_plugin_core::prelude::PluginCoreError::Plugin(e.to_string())
        };

        let shader = lua.create_table().map_err(err)?;

        // uniform{ source = "…" } | uniform{ path = "…" } -> table | nil
        //
        // `nil` for "this shader binds nothing in the material's group", because that is an
        // answer and not a failure — a caller forced to pcall for it ends up swallowing real
        // errors alongside it. A shader that HAS a group but whose parameter block could not
        // be laid out comes back with `resources` and no `fields`: still worth knowing.
        let f = lua
            .create_function(|lua_ctx, cfg: Table| {
                let text = read_source(&cfg, "arbor.shader.uniform")?;

                let Some(g) = material_bind_group(&text) else {
                    return Ok(mlua::Value::Nil);
                };

                // Answered before anything is moved out of `g`: `owns_group` borrows the
                // whole group, and moving one field out leaves it partially moved and
                // unborrowable. Reading first is the order that never has to care.
                //
                // Whether the material owns its whole bind group or extends
                // `StandardMaterial` follows from the binding indices, so it is answered here
                // rather than left for every caller to re-derive — and getting it wrong means
                // asking the renderer for the wrong material entirely.
                let owns_group = g.owns_group();

                let out = lua_ctx.create_table()?;
                // Verbatim, and text rather than a number: in a Bevy shader the material's
                // group is `#{MATERIAL_BIND_GROUP}` and there is no number until naga_oil
                // substitutes one.
                out.set("group", g.group.clone())?;
                out.set("owns_group", owns_group)?;

                let block_table = |u: &bennu_wgsl::prelude::UniformBlock| -> mlua::Result<mlua::Table> {
                    let t = lua_ctx.create_table()?;
                    t.set("struct", u.struct_name.clone())?;
                    t.set("variable", u.variable.clone())?;
                    t.set("binding", u.binding)?;
                    t.set("size", u.size)?;
                    let fields = lua_ctx.create_table()?;
                    for (i, f) in u.fields.iter().enumerate() {
                        let row = lua_ctx.create_table()?;
                        row.set("name", f.name.clone())?;
                        row.set("type", f.ty.clone())?;
                        row.set("offset", f.offset)?;
                        row.set("size", f.size)?;
                        // A shape rather than a component count: a matrix is written column by
                        // column and each column is padded on its own, so `mat3x3<f32>` strides
                        // 16 bytes and not 12.
                        row.set("columns", f.columns)?;
                        row.set("rows", f.rows)?;
                        row.set("column_stride", f.column_stride)?;

                        // `// @preview` lines, one per lane. The only place a `vec4` packing
                        // four unrelated quantities can say which is which.
                        if !f.hints.is_empty() {
                            let hints = lua_ctx.create_table()?;
                            for (hi, h) in f.hints.iter().enumerate() {
                                let hr = lua_ctx.create_table()?;
                                hr.set("label", h.label.clone())?;
                                if let Some(v) = h.min { hr.set("min", v)?; }
                                if let Some(v) = h.max { hr.set("max", v)?; }
                                if let Some(v) = h.default { hr.set("default", v)?; }
                                if let Some(v) = &h.hint { hr.set("hint", v.clone())?; }
                                // `#rrggbb` — the lane is a colour and this is where it
                                // starts. A panel cannot work either out: whether a `vec4` is
                                // a colour is guessed from the name, which fails for `hot`,
                                // `deep` and `foam`, and even a correct guess opens on a
                                // palette entry rather than on the colour the author chose.
                                if let Some(v) = &h.hex { hr.set("hex", v.clone())?; }
                                hints.set(hi + 1, hr)?;
                            }
                            // Omitted when empty rather than sent as `{}`: Lua has one table
                            // type, so an empty one crosses as a JSON object and the host's
                            // renderer refuses to iterate it.
                            row.set("hints", hints)?;
                        }
                        fields.set(i + 1, row)?;
                    }
                    t.set("fields", fields)?;
                    Ok(t)
                };

                // Every block, in binding order. A material extension has one PER BINDING from
                // 100 up — `mole_params`, `mole_fur`, `mole_env_a`… — and a caller reading only
                // the first drives one fifth of the material.
                let blocks = lua_ctx.create_table()?;
                for (i, u) in g.blocks.iter().enumerate() {
                    blocks.set(i + 1, block_table(u)?)?;
                }

                if let Some(u) = g.uniform() {
                    // The first block, flat, for a caller that only ever has one.
                    out.set("struct", u.struct_name.clone())?;
                    out.set("variable", u.variable.clone())?;
                    out.set("binding", u.binding)?;
                    out.set("size", u.size)?;
                    // The first block's field list, taken from the table already built for it
                    // rather than built a second time.
                    let first: mlua::Table = blocks.get(1)?;
                    out.set("fields", first.get::<mlua::Table>("fields")?)?;

                    // A stable name for this material, for anything keying saved state by it.
                    // The struct's name when it has one; the variable's when the shader bound a
                    // bare value and there is no struct to name.
                    let key = if u.struct_name.is_empty() {
                        u.variable.clone()
                    } else {
                        u.struct_name.clone()
                    };
                    out.set("key", key)?;
                }

                // Handed over last, because `out.set` MOVES the table and the flat copy above
                // reads a field out of it. Setting it first and borrowing after is the same
                // mistake in a different costume.
                out.set("blocks", blocks)?;

                // Textures and samplers, always — a material can have no parameter block at
                // all, and a panel that only learns about the uniform omits what the pipeline
                // still has to be given.
                let resources = lua_ctx.create_table()?;
                for (i, r) in g.resources.iter().enumerate() {
                    let row = lua_ctx.create_table()?;
                    row.set("binding", r.binding)?;
                    row.set("name", r.name.clone())?;
                    row.set("type", r.ty.clone())?;
                    row.set("kind", r.kind.as_str())?;
                    resources.set(i + 1, row)?;
                }
                out.set("resources", resources)?;

                Ok(mlua::Value::Table(out))
            })
            .map_err(err)?;
        shader.set("uniform", f).map_err(err)?;

        // preview{ source = "…" } | preview{ path = "…" } -> table | nil
        //
        // The shader RENUMBERED onto the fixed layout a previewer has, and the map back.
        //
        // Its own function rather than a field on `uniform`, because it answers a different
        // question. `uniform` says what a material declares — true of the file, useful to a
        // linter, to hover, to a check against the Rust half. This says what a *previewer* has
        // to do to run it, which is only meaningful to something that has slots.
        //
        // The rewrite is a copy. Nothing is written back, and the preview already replaces its
        // shader asset on every keystroke — this changes what is in that copy.
        let g = lua
            .create_function(|lua_ctx, cfg: Table| {
                let text = read_source(&cfg, "arbor.shader.preview")?;
                // Which previewer this is for. The browser viewport declares far fewer texture
                // slots than the headless renderer, because a fragment stage on WebGL2 gets 16
                // texture units in total and the engine has spent most of them — so a shader
                // renumbered for one and drawn by the other lands on slots that are not there.
                // `viewport` is the default: a plugin asking this question is drawing in a
                // panel unless it says otherwise.
                let target: Option<String> = cfg.get("target").ok();
                let caps = match target.as_deref() {
                    Some("native") | Some("render") => PreviewCaps::native(),
                    _ => PreviewCaps::viewport(),
                };
                let Some(plan) = preview_plan_with(&text, caps) else {
                    return Ok(mlua::Value::Nil);
                };

                let out = lua_ctx.create_table()?;
                out.set("source", plan.source.clone())?;
                out.set("group", plan.group.clone())?;
                out.set("owns_group", plan.owns_group)?;
                // Whether anything moved. A shader already written onto the slots renders the
                // same either way, and a caller that knows can skip re-sending the source.
                out.set("rewritten", plan.rewritten())?;
                // Se lo shader porta il proprio `@vertex`. Decide QUALE materiale il visore
                // deve costruire, non come lo disegna: `Material::vertex_shader` è statico.
                out.set("vertex_entry", plan.vertex_entry)?;

                let base = lua_ctx.create_table()?;
                base.set("binding", plan.layout.base)?;
                base.set("uniforms", plan.layout.uniforms)?;
                base.set("textures", plan.layout.textures_2d)?;
                base.set("samplers", plan.layout.samplers)?;
                out.set("layout", base)?;

                let rows = |family: SlotFamily| -> mlua::Result<mlua::Table> {
                    let t = lua_ctx.create_table()?;
                    for (i, p) in plan.family(family).iter().enumerate() {
                        let row = lua_ctx.create_table()?;
                        row.set("name", p.name.clone())?;
                        row.set("type", p.ty.clone())?;
                        row.set("slot", p.slot)?;
                        row.set("from", p.from)?;
                        row.set("to", p.to)?;
                        row.set("aliased", p.aliased)?;
                        if let Some(h) = &p.hint {
                            row.set("hint", h.clone())?;
                        }
                        t.set(i + 1, row)?;
                    }
                    Ok(t)
                };
                out.set("uniforms", rows(SlotFamily::Uniform)?)?;
                // One flat list, in the order the runtime fills its slots: the 2D textures,
                // then the array textures, then the cubes. Flat because the thing on the other
                // end is a list of picture names by position, and three lists would be three ways
                // for the panel and the runtime to disagree about which slot is which.
                let textures = lua_ctx.create_table()?;
                let mut n = 0usize;
                for (family, offset) in [
                    (SlotFamily::Texture2d, 0),
                    (SlotFamily::Texture2dArray, TEXTURE_2D_SLOTS),
                    (SlotFamily::TextureCube, TEXTURE_2D_SLOTS + TEXTURE_2D_ARRAY_SLOTS),
                ] {
                    for p in plan.family(family) {
                        let row = lua_ctx.create_table()?;
                        row.set("name", p.name.clone())?;
                        row.set("type", p.ty.clone())?;
                        row.set("kind", family.as_str())?;
                        // The index into the runtime's FLAT slot list — the 2D slots, then the
                        // array slots, then the cubes. A running counter would be right only
                        // while a shader has 2D textures and nothing else: a cube in a shader
                        // with one texture belongs at 14, not at 1.
                        row.set("index", offset + p.slot)?;
                        row.set("from", p.from)?;
                        row.set("to", p.to)?;
                        // WHAT it is — `diffuse`, `normal`, `pbr`. Textures sharing a key
                        // share a slot on purpose: a preview has no assets, so both would be
                        // handed the same generated picture anyway.
                        row.set("key", p.key.clone())?;
                        // The picture that key opens on, which a panel may override.
                        row.set("image", p.image.clone())?;
                        // True only when the previewer ran OUT of slots for a new kind — a
                        // different and worse thing than sharing one with the same kind.
                        row.set("aliased", p.aliased)?;
                        if let Some(h) = &p.hint {
                            row.set("hint", h.clone())?;
                        }
                        n += 1;
                        textures.set(n, row)?;
                    }
                }
                out.set("textures", textures)?;
                out.set("samplers", rows(SlotFamily::Sampler)?)?;

                // What could not be placed, with the sentence to say so. A caller refuses on
                // this instead of building a pipeline wgpu will not create.
                let rejected = lua_ctx.create_table()?;
                for (i, r) in plan.rejected.iter().enumerate() {
                    let row = lua_ctx.create_table()?;
                    row.set("name", r.name.clone())?;
                    row.set("type", r.ty.clone())?;
                    row.set("binding", r.binding)?;
                    row.set("reason", r.reason.clone())?;
                    rejected.set(i + 1, row)?;
                }
                out.set("rejected", rejected)?;

                Ok(mlua::Value::Table(out))
            })
            .map_err(err)?;
        shader.set("preview", g).map_err(err)?;

        arbor.set("shader", shader).map_err(err)?;
        Ok(())
    }
}

/// The namespaces `bennu-be` hands to its plugin host, on top of the host-pure ones.
pub fn namespaces() -> Vec<Arc<dyn LuaNamespaceInstaller>> {
    vec![Arc::new(ShaderNs)]
}

/// The `arbor.*` installer bennu-be wires at boot.
///
/// The host-pure namespaces plus this crate's own, in that order — `register_lua_api` runs the
/// extras last so they can see everything already published.
pub fn bennu_api_installer() -> Arc<dyn arbor_plugin_core::prelude::LuaApiInstaller> {
    Arc::new(BennuApiInstaller { extra: namespaces() })
}

struct BennuApiInstaller {
    extra: Vec<Arc<dyn LuaNamespaceInstaller>>,
}

impl arbor_plugin_core::prelude::LuaApiInstaller for BennuApiInstaller {
    fn install(
        &self,
        lua: &Lua,
        params: arbor_plugin_core::prelude::ApiInstallParams,
    ) -> arbor_plugin_core::prelude::PluginCoreResult<()> {
        arbor_plugin_core::prelude::register_lua_api(lua, params, &self.extra)
    }
}

/// Canonical entry point for this crate's public API — the workspace convention.
pub mod prelude {
    pub use crate::{bennu_api_installer, namespaces, ShaderNs};
}
