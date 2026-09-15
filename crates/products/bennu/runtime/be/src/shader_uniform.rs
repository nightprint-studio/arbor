//! `bennu_shader_uniform` — what parameters a WGSL material declares.
//!
//! Bennu already reads WGSL properly, for highlighting and for checking a material's Rust half
//! against its shader half. This hands that reading to anyone who wants to **drive** the
//! material rather than navigate it: a preview panel, a live-tuning tool, a test.
//!
//! ## Why this is a handler and not a parser somewhere else
//!
//! The alternative was a parser next to each consumer, and the first two attempts at that both
//! got the same two things wrong: WGSL block comments **nest**, and `@group(0)` is the VIEW's
//! bind group rather than the material's. Both are handled once, in `bennu-wgsl`, by code that
//! also feeds completion and hover — so it stays right because everything else depends on it.
//!
//! The layout is the other half. A uniform buffer is bytes, and writing a value where the
//! shader will read it means honouring WGSL's alignment: a `vec3` is three components wide and
//! sixteen-byte aligned, which is the rule a hand-written packer misses. Getting it wrong is
//! quiet — every value lands in the next field along and the shader renders something
//! plausible that is not what was asked for.

use bennu_core::prelude::BennuState;
use bennu_wgsl::prelude::material_bind_group;
use serde::{Deserialize, Serialize};

/// Where to read the shader from.
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ShaderUniformArgs {
    /// The `.wgsl` file's absolute path. Used only to read it when `source` is absent, so a
    /// caller holding an unsaved buffer can pass the text instead.
    #[serde(default)]
    pub path: Option<String>,
    /// The shader's text. Preferred over `path`: an editor asking about the buffer it is
    /// showing means the buffer, not what happens to be on disk.
    #[serde(default)]
    pub source: Option<String>,
}

/// One member of the material's uniform block.
#[derive(Serialize, schemars::JsonSchema)]
pub struct UniformFieldDto {
    /// The name the shader gave it — which is what a control should be labelled with.
    pub name: String,
    /// Declared type, verbatim: `f32`, `vec4<f32>`, `mat3x3<f32>`.
    pub ty: String,
    /// Byte offset from the start of the block.
    pub offset: u32,
    /// Bytes it occupies, padding included — 48 for a `mat3x3<f32>`, not 36.
    pub size: u32,
    /// Columns × rows: a scalar is 1×1, a `vec3` is 1×3, a `mat4x4` is 4×4. A shape rather
    /// than a component count because a matrix is written column by column, each padded.
    pub columns: u32,
    pub rows: u32,
    /// Bytes from one column to the next — 16 for a `mat3x3<f32>`, not 12.
    pub column_stride: u32,
    /// What the shader's author wrote about this member in `// @preview` lines above the
    /// declaration, one entry per lane. Empty when the shader has none.
    ///
    /// The only place a `vec4` packing four unrelated quantities can say which is which: WGSL
    /// gives the four of them one name, and has no room for an attribute carrying more.
    pub hints: Vec<PreviewHintDto>,
}

/// One `// @preview` line: what a lane is called, and what it ranges over.
#[derive(Serialize, schemars::JsonSchema)]
pub struct PreviewHintDto {
    /// The name to use instead of `X`/`Y`/`Z`/`W`. Empty when the line only set a range.
    pub label: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// The value to open on, when the author named one.
    pub default: Option<f32>,
    /// A sentence for the control's tooltip.
    pub hint: Option<String>,
}

/// A binding in the material's group that is not the parameter block: a texture, a sampler, a
/// storage buffer.
///
/// Reported because a caller that only learns about the uniform will build a form omitting
/// what the material also needs — and a pipeline with an unbound texture does not run.
#[derive(Serialize, schemars::JsonSchema)]
pub struct ResourceDto {
    pub binding: u32,
    pub name: String,
    /// Declared type, verbatim: `texture_2d<f32>`, `sampler`.
    pub ty: String,
    /// `texture` · `storage_texture` · `sampler` · `storage` · `other`.
    pub kind: String,
}

/// One parameter block: a struct the shader declared, or a lone value it bound.
#[derive(Serialize, schemars::JsonSchema)]
pub struct UniformBlockDto {
    /// The struct's name. Empty when the shader bound a bare value (`var<uniform> p: vec4<f32>`)
    /// and there is no struct to name.
    pub struct_name: String,
    /// The variable it is bound to — the only name a bare-value block has.
    pub variable: String,
    pub binding: u32,
    pub size: u32,
    pub fields: Vec<UniformFieldDto>,
}

/// The material's parameter block, or `bound: false` when there is none to describe.
#[derive(Serialize, schemars::JsonSchema)]
pub struct ShaderUniform {
    /// False when the shader binds no parameter block this can describe — it has none, or it
    /// hides one behind an `#import`, or a member's type is not one whose layout is known.
    /// Reported as "no" rather than as a partial answer: a form built from half a struct
    /// writes into the wrong offsets.
    pub bound: bool,
    /// The struct's name, as written.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub struct_name: String,
    /// The variable it is bound to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub variable: String,
    /// The group expression verbatim — `"#{MATERIAL_BIND_GROUP}"`, `"2"`. Text and not a
    /// number because in a Bevy shader there is no number until naga_oil substitutes one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    pub binding: u32,
    /// Size of the whole block in bytes, rounded up to 16 as a uniform buffer is.
    pub size: u32,
    pub fields: Vec<UniformFieldDto>,
    /// True when the material owns its whole bind group — its own struct at a low binding,
    /// with no `StandardMaterial` underneath. False when it EXTENDS one, which by Bevy's
    /// convention means its own uniforms start at binding 100. Which one a shader is decides
    /// which material can render it, and it follows from the shader rather than being a choice.
    pub owns_group: bool,
    /// Every parameter block in the group, in binding order.
    ///
    /// A material extension does not have one block: it declares a separate `var<uniform>` per
    /// binding from 100 up. The flat fields above are the FIRST of these — enough for a
    /// material that owns its group, and one fifth of a material that does not.
    pub blocks: Vec<UniformBlockDto>,
    /// Textures, samplers and storage buffers in the same group, in binding order. Present
    /// even when `bound` is false: a material can be textures only, and one whose parameter
    /// block could not be described still has to have its textures supplied.
    pub resources: Vec<ResourceDto>,
}

impl ShaderUniform {
    fn none() -> Self {
        Self {
            bound: false,
            struct_name: String::new(),
            variable: String::new(),
            group: String::new(),
            binding: 0,
            size: 0,
            fields: Vec::new(),
            resources: Vec::new(),
            owns_group: false,
            blocks: Vec::new(),
        }
    }
}

fn block_dto(u: &bennu_wgsl::prelude::UniformBlock) -> UniformBlockDto {
    UniformBlockDto {
        struct_name: u.struct_name.clone(),
        variable: u.variable.clone(),
        binding: u.binding,
        size: u.size,
        fields: u.fields.iter().map(field_dto).collect(),
    }
}

fn field_dto(f: &bennu_wgsl::prelude::UniformField) -> UniformFieldDto {
    UniformFieldDto {
        name: f.name.clone(),
        ty: f.ty.clone(),
        offset: f.offset,
        size: f.size,
        columns: f.columns,
        rows: f.rows,
        column_stride: f.column_stride,
        hints: f
            .hints
            .iter()
            .map(|h| PreviewHintDto {
                label: h.label.clone(),
                min: h.min,
                max: h.max,
                default: h.default,
                hint: h.hint.clone(),
            })
            .collect(),
    }
}

/// Everything the handler answers, from a source string. Split out so the tests exercise the
/// real mapping rather than a copy of it.
fn describe(source: &str) -> ShaderUniform {
    let Some(g) = material_bind_group(source) else { return ShaderUniform::none() };
    // Everything that reads the WHOLE group is answered first. `owns_group` borrows all of
    // `g`, and moving a field out — the resources, the group string — leaves it partially
    // moved and unborrowable. Reading before dismantling is the order that never has to care.
    let owns_group = g.owns_group();
    let blocks: Vec<UniformBlockDto> = g.blocks.iter().map(block_dto).collect();
    let group = g.group.clone();
    let first = g.blocks.first().map(block_dto);

    let resources = g
        .resources
        .into_iter()
        .map(|r| ResourceDto {
            binding: r.binding,
            name: r.name,
            ty: r.ty,
            kind: r.kind.as_str().to_string(),
        })
        .collect();
    match first {
        None => ShaderUniform { group, resources, blocks, owns_group, ..ShaderUniform::none() },
        Some(u) => ShaderUniform {
            bound: true,
            struct_name: u.struct_name,
            variable: u.variable,
            group,
            binding: u.binding,
            size: u.size,
            fields: u.fields,
            resources,
            owns_group,
            blocks,
        },
    }
}

/// The parameters a WGSL material declares, with the byte offsets to write them at.
#[arbor_rpc::handler(mcp(
    title = "WGSL material parameters",
    safety = read,
    description = "Read the uniform block a WGSL material binds: its struct name, its fields \
                   with their types, and the byte offset of each — enough to build a form for \
                   the material or to write its uniform buffer.",
))]
fn bennu_shader_uniform(
    _ctx: &BennuState,
    args: ShaderUniformArgs,
) -> Result<ShaderUniform, String> {
    let source = match (args.source, args.path) {
        (Some(text), _) => text,
        (None, Some(path)) => {
            // A `.wgsl` is UTF-8 by language definition, which is why this reads it directly
            // rather than through bennu's encoding-aware reader: that one needs a project root
            // to resolve an override against, and a caller asking about one shader has no
            // reason to have opened a project.
            std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?
        }
        (None, None) => return Err("bennu_shader_uniform: pass `source` or `path`".to_string()),
    };

    Ok(describe(&source))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPIRAL: &str = r#"
        struct SpiralHoverParams {
            sand_color: vec4<f32>,
            spiral_speed: f32,
            spiral_arms: f32,
        };
        @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: SpiralHoverParams;
    "#;

    #[test]
    fn a_material_reports_its_named_parameters() {
        let u = describe(SPIRAL);
        assert!(u.bound);
        assert_eq!(u.struct_name, "SpiralHoverParams");
        assert_eq!(u.fields.len(), 3);
        assert_eq!(u.fields[1].name, "spiral_speed");
        assert_eq!(u.fields[1].offset, 16);
    }

    #[test]
    fn a_shader_without_one_says_so_instead_of_erroring() {
        // Not an error: "this shader takes no parameters" is a perfectly good answer, and a
        // caller that has to catch an exception for it will end up ignoring real failures too.
        let u = describe("@fragment fn fragment() -> @location(0) vec4<f32> { return vec4(1.0); }");
        assert!(!u.bound);
        assert!(u.fields.is_empty());
        assert!(u.resources.is_empty());
    }

    #[test]
    fn a_materials_textures_travel_with_its_parameters() {
        // The half that was missing: a panel built from the uniform alone omits what else the
        // material needs, and the pipeline will not run without it.
        let src = r#"
            struct P { tint: vec4<f32>, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
            @group(#{MATERIAL_BIND_GROUP}) @binding(1) var base: texture_2d<f32>;
            @group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_sampler: sampler;
        "#;
        let u = describe(src);
        assert!(u.bound);
        assert_eq!(u.resources.len(), 2);
        assert_eq!(u.resources[0].kind, "texture");
        assert_eq!(u.resources[1].kind, "sampler");
    }

    #[test]
    fn a_textures_only_material_still_reports_them() {
        let src = r#"
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var tex: texture_2d<f32>;
            @group(#{MATERIAL_BIND_GROUP}) @binding(1) var samp: sampler;
        "#;
        let u = describe(src);
        assert!(!u.bound);
        assert_eq!(u.resources.len(), 2);
    }
}
