//! Geometry for a shader render, from whatever package makes it.
//!
//! ## Why the render tool has to reach the extensions
//!
//! A shader is written **against** geometry. Grass that only grows on upward faces, a tile
//! whose sides are meant to stay bare, a flame whose UV runs along its length — every one of
//! those is a claim about the mesh, and a sphere agrees with none of them. So a render on a
//! primitive answers a question nobody asked: the material looks fine, and the bug that was
//! being hunted lives in the coupling between the two.
//!
//! The panel has had this since the picker started reading the `mesh-source` catalogue. The
//! tool did not, and worse, it did not say so — an unknown name fell through a `match` onto
//! `Sphere`, so `mesh: "fulcrum/flame"` rendered a sphere and reported success. That is the
//! failure mode a diagnostic tool must not have: it does not look like an error, it looks
//! like the shader being wrong.
//!
//! ## Where the vertices come from
//!
//! The shell, over the reverse channel. Extensions run in the process that owns the wasm
//! engine, which is not this one — the same reason `arbor.ext.call` is a host call rather
//! than an engine per backend. The reply is a `mesh-data`: flat float lists, exactly what the
//! runtime's `MeshSpec::Raw` wants, so nothing here reshapes geometry it does not understand.
//!
//! ## Addressing
//!
//! `<provider>/<shape>`, as the WIT says a host addresses a mesh — two packages offering
//! `cube` do not collide. A bare `shape` is accepted too, because that is what a saved look
//! and the older panels hold; it is resolved by asking each installed package what it offers,
//! which is also what makes the "no such mesh" error able to list the real alternatives.

use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::Value as Json;

/// The shapes the runtime builds itself, with no package involved.
pub const PRIMITIVES: &[&str] = &["sphere", "cube", "plane", "torus", "capsule", "cylinder"];

pub fn is_primitive(name: &str) -> bool {
    PRIMITIVES.contains(&name)
}

/// One row of the shell's extension surface — the two fields this cares about.
#[derive(Deserialize)]
struct ExtRow {
    interface: String,
    id: String,
}

/// One entry of a package's `catalogue()` — the id is all that is addressed by; the label
/// and the schema belong to the panel, which draws controls from them.
#[derive(Deserialize)]
struct MeshKind {
    id: String,
}

/// Vertex arrays, as a `mesh-source` produced them.
///
/// Deserialised and re-serialised rather than passed through, so a package that answers with
/// something that is not a mesh is caught here — by the thing that knows what a mesh is —
/// instead of becoming a renderer that silently draws nothing.
#[derive(Deserialize, serde::Serialize)]
pub struct MeshData {
    pub positions: Vec<f32>,
    #[serde(default)]
    pub normals: Vec<f32>,
    #[serde(default)]
    pub uvs: Vec<f32>,
    #[serde(default)]
    pub indices: Vec<u32>,
}

/// Every installed `mesh-source` package, in catalogue order.
fn providers(ctx: &BennuState) -> Result<Vec<String>, String> {
    let rows = ctx.host_call("__ext_surface", serde_json::json!({}))?;
    let rows: Vec<ExtRow> = serde_json::from_value(rows)
        .map_err(|e| format!("could not read the installed extensions: {e}"))?;
    Ok(rows
        .into_iter()
        .filter(|r| r.interface == "mesh-source")
        .map(|r| r.id)
        .collect())
}

fn catalogue(ctx: &BennuState, provider: &str) -> Result<Vec<MeshKind>, String> {
    let out = call(ctx, provider, "catalogue", vec![])?;
    serde_json::from_value(out)
        .map_err(|e| format!("'{provider}' returned a catalogue that could not be read: {e}"))
}

fn call(ctx: &BennuState, provider: &str, method: &str, args: Vec<Json>) -> Result<Json, String> {
    ctx.host_call(
        "__ext_call",
        serde_json::json!({
            "plugin": "bennu_shader_render",
            "spec": {
                "interface": "mesh-source",
                "version": 1,
                "id": provider,
                "method": method,
                "args": args,
            },
        }),
    )
}

/// Split `provider/shape`, or `None` for a bare shape name.
fn split(id: &str) -> Option<(&str, &str)> {
    id.split_once('/').filter(|(p, s)| !p.is_empty() && !s.is_empty())
}

/// Everything installed, as one line, for an error that can be acted on.
fn available(ctx: &BennuState) -> String {
    let mut names: Vec<String> = PRIMITIVES.iter().map(|s| (*s).to_string()).collect();
    if let Ok(ps) = providers(ctx) {
        for p in ps {
            match catalogue(ctx, &p) {
                Ok(kinds) => names.extend(kinds.into_iter().map(|k| format!("{p}/{}", k.id))),
                // A package that will not answer is named anyway: "installed but broken" and
                // "not installed" send the reader somewhere different.
                Err(_) => names.push(format!("{p}/… (this package would not answer)")),
            }
        }
    }
    names.join(", ")
}

/// Resolve `mesh` to a package and a shape, or say what is on offer.
fn resolve(ctx: &BennuState, mesh: &str) -> Result<(String, String), String> {
    let installed = providers(ctx)?;
    if installed.is_empty() {
        return Err(format!(
            "'{mesh}' is not one of the built-in shapes ({}), and no mesh-source package is \
             installed to provide it. A package that generates geometry — Fulcrum's, or your \
             own — appears here as soon as it is installed and enabled.",
            PRIMITIVES.join(", "),
        ));
    }

    if let Some((provider, shape)) = split(mesh) {
        if !installed.iter().any(|p| p == provider) {
            return Err(format!(
                "no mesh-source package called '{provider}' is installed. Available: {}",
                available(ctx),
            ));
        }
        // The shape is NOT checked against the catalogue here: `build` is the authority on
        // what it accepts, and its own error names the id. Asking twice would make a package
        // whose catalogue and builder disagree fail with the wrong sentence.
        return Ok((provider.to_string(), shape.to_string()));
    }

    // Bare name — first package that offers it wins, in catalogue order, which is the same
    // rule the panel's picker follows.
    for provider in &installed {
        if let Ok(kinds) = catalogue(ctx, provider) {
            if kinds.iter().any(|k| k.id == mesh) {
                return Ok((provider.clone(), mesh.to_string()));
            }
        }
    }

    Err(format!(
        "no mesh called '{mesh}'. Available: {}",
        available(ctx),
    ))
}

/// Build the geometry named by `mesh`, with `params` from its schema.
///
/// `Ok(None)` means the name was a built-in the runtime makes itself; the caller passes it
/// through as `--mesh`. `Ok(Some(_))` is a package's vertices, for `--mesh-file`.
pub fn build(
    ctx: &BennuState,
    mesh: &str,
    params: Option<&Json>,
) -> Result<Option<MeshData>, String> {
    if is_primitive(mesh) {
        if params.is_some_and(|p| !p.is_null() && p != &serde_json::json!({})) {
            return Err(format!(
                "'{mesh}' is a built-in shape and takes no parameters. Parameters belong to a \
                 mesh from a package, whose schema declares them — `{}` and the like.",
                available(ctx),
            ));
        }
        return Ok(None);
    }

    let (provider, shape) = resolve(ctx, mesh)?;
    // A JSON *string*, because that is what `build(id: string, params: string)` takes: the
    // schema is the package's, so the parameters cross as text it parses itself rather than
    // as a shape the host would have to agree with.
    let params_text = params
        .filter(|p| !p.is_null())
        .map(|p| p.to_string())
        .unwrap_or_else(|| "{}".to_string());

    let out = call(
        ctx,
        &provider,
        "build",
        vec![Json::String(shape.clone()), Json::String(params_text)],
    )
    .map_err(|e| format!("{provider}/{shape}: {e}"))?;

    let data: MeshData = serde_json::from_value(out)
        .map_err(|e| format!("{provider}/{shape} did not return a mesh: {e}"))?;
    if data.positions.is_empty() {
        return Err(format!(
            "{provider}/{shape} built an empty mesh — nothing would be drawn. Check the \
             parameters against the shape's schema."
        ));
    }
    Ok(Some(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qualified_id_splits_into_package_and_shape() {
        assert_eq!(split("fulcrum/flame"), Some(("fulcrum", "flame")));
    }

    #[test]
    fn a_bare_name_is_not_a_split() {
        // It is a shape to look up, not a package with no shape — the difference decides
        // which error the caller gets when it resolves to nothing.
        assert_eq!(split("sphere"), None);
        assert_eq!(split("/sphere"), None);
        assert_eq!(split("fulcrum/"), None);
    }

    #[test]
    fn the_built_ins_are_the_ones_the_runtime_builds() {
        assert!(is_primitive("torus"));
        assert!(!is_primitive("fulcrum/flame"));
        // Capsule and cylinder are in the runtime's `Primitive` enum but were never in the
        // tool's doc list; leaving them out here would send them down the extension path and
        // fail with "no mesh called 'capsule'".
        assert!(is_primitive("capsule"));
        assert!(is_primitive("cylinder"));
    }
}
