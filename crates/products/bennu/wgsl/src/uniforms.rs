//! The material's bind group: the parameter block, and the resources beside it.
//!
//! Composed from the two scans next door rather than parsed again — [`crate::bindings::scan`]
//! knows which `@group` is the material's, [`crate::symbols::scan`] knows a struct's members
//! and their types, and both already read past nested block comments. What is added here is
//! what neither of them owes anybody: **byte offsets**, and the distinction between a
//! parameter and a resource.
//!
//! ## Why a material is two things
//!
//! A parameter block is values you write into a buffer. A texture is a file you bind. Both
//! live in the same `@group`, and a caller that only learns about the first will build a panel
//! that silently omits half of what the material needs — and then wonder why the shader will
//! not run, because a pipeline with an unbound texture does not.
//!
//! So both are reported, separately, because what you *do* with them is different.
//!
//! ## Why offsets belong in this crate
//!
//! A uniform buffer is bytes. To hand a material new values you have to write each one where
//! the shader will read it, and where that is depends on WGSL's alignment rules — a `vec4`
//! aligns to 16, a `vec3` **also** to 16, a `vec2` to 8, a scalar to 4, and a `mat3x3<f32>` is
//! three columns each padded out to 16, so it occupies 48 bytes rather than the 36 its
//! components add up to.
//!
//! Anyone driving a material needs that layout, and computing it from a list of type names is
//! the kind of thing that is written subtly wrong once per consumer. Getting it wrong is
//! quiet: every value lands in the next field along, and the shader renders something
//! plausible that is not what you asked for. So it is computed here, beside the scan that
//! produced the types.
//!
//! ## What this deliberately does not do
//!
//! It does not resolve `#import`ed structs, it does not evaluate `const` array lengths, and it
//! does not descend into a nested struct member. Any of those makes every offset after it a
//! guess, so the parameter block is reported as absent rather than short — a form built from
//! half a struct writes into the wrong places.

use crate::bindings::{scan as scan_bindings, Binding};
use crate::preview_hints::{hints_before, PreviewHint};
use crate::symbols::{scan as scan_symbols, WgslSymbolKind};

/// What a binding in the material's group IS, which decides what a caller can do with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// `texture_2d<f32>`, `texture_cube<f32>`, `texture_2d_array<f32>`, …
    Texture,
    /// `texture_storage_2d<rgba8unorm, write>`, …
    StorageTexture,
    /// `sampler` or `sampler_comparison`.
    Sampler,
    /// A `var<storage>` buffer.
    Storage,
    /// A uniform whose type is an array — `array<vec4<f32>, 32>`.
    ///
    /// A parameter block in every sense except that it has no fields to lay out and no panel
    /// could offer it: 32 elements is 128 numbers, and the reason a shader declares one is
    /// that the values come from the world each frame — a list of nearby lights, a bone
    /// palette — rather than from somebody turning a knob. Named rather than lumped under
    /// `Other`, so a caller can say "supplied at runtime" instead of "unclassified".
    UniformArray,
    /// Something in the material's group this crate does not classify. Reported anyway: a
    /// caller is better off knowing the material has a binding it must supply than believing
    /// the group is complete.
    Other,
}

impl ResourceKind {
    fn of(ty: &str, address_space: &str) -> Self {
        let t = ty.trim();
        if address_space.starts_with("storage") {
            return Self::Storage;
        }
        if t.starts_with("texture_storage") {
            return Self::StorageTexture;
        }
        if t.starts_with("texture_") {
            return Self::Texture;
        }
        if t == "sampler" || t == "sampler_comparison" {
            return Self::Sampler;
        }
        if t.starts_with("array<") {
            return Self::UniformArray;
        }
        Self::Other
    }

    /// The word a caller shows or matches on.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::StorageTexture => "storage_texture",
            Self::Sampler => "sampler",
            Self::Storage => "storage",
            Self::UniformArray => "uniform_array",
            Self::Other => "other",
        }
    }
}

/// A binding in the material's group that is not the parameter block.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialResource {
    pub binding: u32,
    pub name: String,
    /// The declared type, verbatim.
    pub ty: String,
    pub kind: ResourceKind,
}

/// One member of the parameter block.
#[derive(Debug, Clone, PartialEq)]
pub struct UniformField {
    pub name: String,
    /// The declared type, verbatim: `f32`, `vec4<f32>`, `mat3x3<f32>`.
    pub ty: String,
    /// Byte offset from the start of the block.
    pub offset: u32,
    /// Bytes this member occupies, padding included — 48 for a `mat3x3<f32>`.
    pub size: u32,
    /// Columns × rows. A scalar is `1 × 1`, a `vec3` is `1 × 3`, a `mat4x4` is `4 × 4`.
    ///
    /// Given as a shape rather than a component count because writing a matrix means writing
    /// it column by column, and each column is padded on its own.
    pub columns: u32,
    pub rows: u32,
    /// Bytes from one column to the next. Equals `size` for a scalar or a vector; for a
    /// `mat3x3<f32>` it is 16 and not 12, which is the rule a hand-written packer misses.
    pub column_stride: u32,
    /// What the shader's author said about this member, one entry per lane, from the
    /// `// @preview` lines above the declaration. Empty when there are none — which is every
    /// shader written before the convention, and the reason the convention costs nothing.
    ///
    /// This is the only place a `vec4` packing four unrelated quantities can say so: WGSL has
    /// one name for the four of them and no room for an attribute that would carry more.
    pub hints: Vec<PreviewHint>,
}

impl UniformField {
    /// True when this is a single scalar or vector — the members a panel can put on a control
    /// without deciding what a matrix widget looks like.
    pub fn is_simple(&self) -> bool {
        self.columns == 1
    }
}

/// The parameter block a material binds.
#[derive(Debug, Clone, PartialEq)]
pub struct UniformBlock {
    /// The struct's name, as the shader wrote it.
    pub struct_name: String,
    /// The variable it is bound to.
    pub variable: String,
    pub binding: u32,
    /// Size of the whole block in bytes, rounded up to 16 as a uniform buffer is.
    pub size: u32,
    pub fields: Vec<UniformField>,
}

/// Everything a material's bind group asks the pipeline to supply.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialBindGroup {
    /// The group expression verbatim — `"#{MATERIAL_BIND_GROUP}"`, `"2"`. Text and not a
    /// number because in a Bevy shader there is no number until naga_oil substitutes one.
    pub group: String,
    /// Every parameter block the group binds, in binding order.
    ///
    /// A list and not one block, because a Bevy material **extension** does not have one: it
    /// declares a separate `var<uniform>` per binding from 100 up — `mole_params`, `mole_fur`,
    /// `mole_env_a`… — and reading only the lowest reported the other four as unbound
    /// resources, which is both wrong and the reason a preview of them rendered nothing.
    ///
    /// A material that owns its whole bind group has exactly one, at binding 0.
    pub blocks: Vec<UniformBlock>,
    /// Textures, samplers and storage buffers, in binding order.
    pub resources: Vec<MaterialResource>,
}

impl MaterialBindGroup {
    /// True when there is nothing here for a caller to do.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.resources.is_empty()
    }

    /// The first block — what a material owning its whole bind group has exactly one of.
    ///
    /// Kept for the callers that only ever want that one; anything driving a material
    /// EXTENSION has to walk [`Self::blocks`], because there the parameters are spread across
    /// one binding each.
    pub fn uniform(&self) -> Option<&UniformBlock> {
        self.blocks.first()
    }

    /// The bindings that are not values — textures, samplers, storage buffers.
    ///
    /// A panel cannot put any of them on a control: they are GPU resources, and the layout
    /// that carries them is decided when the previewing material is COMPILED, not when a
    /// shader is opened. A previewer offering a buffer at every index cannot become one
    /// offering a sampler at 101 because this particular shader wants one there.
    ///
    /// That used to make them a refusal. It no longer does: [`crate::preview_layout`] moves
    /// the **shader** onto the slots a previewer has instead, so a texture and a sampler are
    /// ordinary. What this still answers is "which of a material's inputs are not numbers",
    /// which is a different and still useful question — a panel lists them apart from the
    /// parameters, because what you do with one is not turn a knob.
    ///
    /// An `array<…>` uniform is NOT one of these: it is still a buffer, and a buffer of zeros
    /// is a legal thing to give it.
    pub fn unsupplied(&self) -> Vec<&MaterialResource> {
        self.resources
            .iter()
            .filter(|r| {
                matches!(
                    r.kind,
                    ResourceKind::Texture | ResourceKind::StorageTexture
                        | ResourceKind::Sampler | ResourceKind::Storage
                )
            })
            .collect()
    }

    /// Whether the material owns its whole bind group, rather than extending
    /// `StandardMaterial`.
    ///
    /// The Bevy convention is the line: an extension's own uniforms start at **binding 100**,
    /// leaving the lower indices to the `StandardMaterial` underneath. A material that binds
    /// below that has no PBR under it and is rendered as itself. It follows from the shader,
    /// which is why it is read here rather than offered as a setting for somebody to get wrong.
    pub fn owns_group(&self) -> bool {
        self.blocks.first().map(|b| b.binding < 100).unwrap_or(false)
    }
}

/// Columns, rows, alignment and column stride for a type a parameter block can hold.
///
/// Deliberately short of exhaustive. An `array<…>` needs its length evaluated and a nested
/// struct needs its own layout walked; admitting either with a guessed size would corrupt
/// every offset after it, so an unknown type refuses the whole block instead.
fn shape(ty: &str) -> Option<(u32, u32, u32, u32)> {
    let t = ty.replace(' ', "");

    // Scalars.
    if matches!(t.as_str(), "f32" | "i32" | "u32" | "f16") {
        let sz = if t == "f16" { 2 } else { 4 };
        return Some((1, 1, sz, sz));
    }

    // `vecN<T>` — a vec3 aligns to 16, which is the gap most packers miss.
    if let Some(rows) = t.strip_prefix("vec").and_then(|r| r.as_bytes().first().map(|b| b - b'0')) {
        if (2..=4).contains(&rows) && t.contains('<') {
            let rows = u32::from(rows);
            let align = if rows == 2 { 8 } else { 16 };
            return Some((1, rows, align, rows * 4));
        }
    }

    // `matCxR<T>` — C columns, each a vecR, each padded to the vector's own alignment. A
    // `mat3x3<f32>` is therefore 48 bytes and not 36.
    if let Some(rest) = t.strip_prefix("mat") {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[1] == b'x' && b[0].is_ascii_digit() && b[2].is_ascii_digit() {
            let cols = u32::from(b[0] - b'0');
            let rows = u32::from(b[2] - b'0');
            if (2..=4).contains(&cols) && (2..=4).contains(&rows) {
                let col_align = if rows == 2 { 8 } else { 16 };
                return Some((cols, rows, col_align, col_align));
            }
        }
    }

    None
}

/// The material's bind group, or `None` when the shader declares nothing in one.
pub fn material_bind_group(source: &str) -> Option<MaterialBindGroup> {
    let bindings = scan_bindings(source);
    let in_group: Vec<&Binding> = bindings.iter().filter(|b| b.in_material_group()).collect();
    if in_group.is_empty() {
        return None;
    }
    let group = in_group[0].group.clone();

    // The parameter block is the lowest-indexed binding whose type is one this crate can lay
    // out — a struct it can walk, or a value type it can size. Lowest because a material with
    // both a block and resources writes the block first by overwhelming convention.
    //
    // A `texture_2d<f32>` and a `sampler` are neither: the first is not a value type, and the
    // second is a plain identifier that names no struct. Both fall through to `resources`,
    // which is where they belong.
    let mut blocks: Vec<UniformBlock> = in_group
        .iter()
        // `var<uniform>` and nothing else. A `var<storage>` binding is a struct name too, so
        // without the address space it read as a parameter block — and a previewer that
        // believed it would hand a uniform buffer to a binding declared as storage, which
        // `create_render_pipeline` refuses.
        .filter(|b| b.address_space == "uniform")
        .filter(|b| is_plain_ident(&b.ty) || shape(&b.ty).is_some())
        .filter_map(|b| read_block(source, b))
        .collect();
    blocks.sort_by_key(|b| b.binding);

    // Everything in the group except the block itself — identified by its INDEX and nothing
    // else. Filtering on "the type is not a plain identifier" looked equivalent and is not:
    // `sampler` is a plain identifier, so that test quietly dropped every sampler a material
    // declared. It also gets the other case right for free — a struct-typed binding whose
    // layout could not be read is still a binding the pipeline has to fill, so it is reported
    // as a resource rather than vanishing.
    let resources = in_group
        .iter()
        .filter(|b| !blocks.iter().any(|u| u.binding == b.index))
        .filter(|b| !b.ty.is_empty())
        .map(|b| MaterialResource {
            binding: b.index,
            name: b.name.clone(),
            ty: b.ty.clone(),
            kind: ResourceKind::of(&b.ty, &b.address_space),
        })
        .collect();

    let out = MaterialBindGroup { group, blocks, resources };
    (!out.is_empty()).then_some(out)
}

/// The first parameter block alone, for callers that only want the values.
pub fn material_uniform(source: &str) -> Option<UniformBlock> {
    material_bind_group(source)?.blocks.into_iter().next()
}

fn is_plain_ident(ty: &str) -> bool {
    !ty.is_empty() && ty.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Lay out whatever the binding's type is: a struct's members, or a lone value.
///
/// ## Why a lone value is a block too
///
/// A Bevy material extension conventionally binds its parameters at index 100 and up, and the
/// smallest form of that is a bare `var<uniform> rock_params: vec4<f32>` — no struct anywhere.
/// Reading only structs reported those materials as having **no parameter block** while their
/// one binding sat in the "also bound" list beside the textures, which is wrong twice: there is
/// a block, and it is not a resource.
///
/// The synthesised field takes the VARIABLE's name, because that is the only name the shader
/// gave it and the one a control should be labelled with.
fn read_block(source: &str, binding: &Binding) -> Option<UniformBlock> {
    if let Some((columns, rows, align, column_stride)) = shape(&binding.ty) {
        let size = columns * column_stride;
        return Some(UniformBlock {
            // No struct was written, so there is no struct name to report. Empty rather than
            // invented: a caller keying saved values by struct name must not be handed one the
            // shader does not have.
            struct_name: String::new(),
            variable: binding.name.clone(),
            binding: binding.index,
            size: size.div_ceil(align) * align,
            fields: vec![UniformField {
                name: binding.name.clone(),
                ty: binding.ty.clone(),
                offset: 0,
                size,
                columns,
                rows,
                column_stride,
                // One entry per lane: a bare `vec4` is where the author has nowhere else to
                // say that `.x` is a frequency and `.y` is an amount.
                hints: hints_before(source, binding.start),
            }],
        });
    }

    let symbols = scan_symbols(source);
    let mut fields = Vec::new();
    let mut offset = 0u32;
    let mut max_align = 4u32;

    for s in symbols.iter().filter(|s| {
        s.kind == WgslSymbolKind::Field && s.container.as_deref() == Some(binding.ty.as_str())
    }) {
        // A field's `detail` is its type — see `symbols::fields_of`, which puts the type there
        // precisely because it is the half a material has to agree with byte for byte.
        let (columns, rows, align, column_stride) = shape(&s.detail)?;
        max_align = max_align.max(align);
        offset = offset.div_ceil(align) * align;
        let size = columns * column_stride;
        fields.push(UniformField {
            name: s.name.clone(),
            ty: s.detail.clone(),
            offset,
            size,
            columns,
            rows,
            column_stride,
            hints: hints_before(source, s.start),
        });
        offset += size;
    }
    if fields.is_empty() {
        return None;
    }

    Some(UniformBlock {
        struct_name: binding.ty.clone(),
        variable: binding.name.clone(),
        binding: binding.index,
        // A struct's size rounds up to its own strictest alignment, which for anything holding
        // a vector or a matrix is 16.
        size: offset.div_ceil(max_align) * max_align,
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Bevy material extension: five bare `vec4`s, one per binding from 100 up, and no
    /// struct anywhere. Reading only structs called this "no parameter block" and listed all
    /// five as unbound resources — and previewing it against a material offering four slots
    /// failed pipeline validation on binding 104.
    const MOLE: &str = r#"
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> mole_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var<uniform> mole_fur: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104)
var<uniform> mole_env_c: vec4<f32>;
"#;

    #[test]
    fn a_texture_material_is_reported_as_unsuppliable() {
        let src = r#"
@group(3) @binding(100) var albedo: texture_2d<f32>;
@group(3) @binding(101) var tile_sampler: sampler;
@group(3) @binding(103) var<uniform> params: vec4<f32>;
"#;
        let g = material_bind_group(src).unwrap();
        let blocked = g.unsupplied();
        assert_eq!(blocked.len(), 2, "the texture and the sampler: {blocked:?}");
        assert!(blocked.iter().any(|r| r.name == "tile_sampler"));
    }

    #[test]
    fn an_array_uniform_is_not_unsuppliable() {
        let src = r#"
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> glow_pos: array<vec4<f32>, 32>;
"#;
        let g = material_bind_group(src).unwrap();
        assert!(g.unsupplied().is_empty(), "a buffer of zeros is a legal thing to give it");
    }

    #[test]
    fn an_array_uniform_is_named_rather_than_unclassified() {
        let src = r#"
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102)
var<uniform> glow_pos: array<vec4<f32>, 32>;
"#;
        let g = material_bind_group(src).unwrap();
        assert_eq!(g.blocks.len(), 1, "the array is not a block this can lay out");
        assert_eq!(g.resources.len(), 1);
        assert_eq!(g.resources[0].kind.as_str(), "uniform_array");
        assert_eq!(g.resources[0].name, "glow_pos");
    }

    #[test]
    fn a_bare_value_binding_is_a_parameter_block() {
        let g = material_bind_group(MOLE).expect("the bindings are right there");
        assert_eq!(g.blocks.len(), 3, "one block per binding: {:?}", g.blocks);
        assert!(g.resources.is_empty(), "none of them is a resource: {:?}", g.resources);
    }

    #[test]
    fn a_bare_value_block_is_named_after_its_variable() {
        let g = material_bind_group(MOLE).unwrap();
        let first = g.uniform().unwrap();
        assert_eq!(first.struct_name, "", "there is no struct to name");
        assert_eq!(first.variable, "mole_params");
        assert_eq!(first.fields.len(), 1);
        assert_eq!(first.fields[0].name, "mole_params");
        assert_eq!(first.fields[0].rows, 4);
    }

    #[test]
    fn blocks_come_back_in_binding_order_with_their_bindings() {
        let g = material_bind_group(MOLE).unwrap();
        let bindings: Vec<u32> = g.blocks.iter().map(|b| b.binding).collect();
        assert_eq!(bindings, vec![100, 101, 104], "the gap at 102/103 must survive");
    }

    const ANNOTATED: &str = r#"
// @preview grain_freq 0.2..8 = 1.6
// @preview albedo_splotch 0..1 = 0.45
// @preview bump 0..1 = 0.65
// @preview band_freq 0..2 = 0.22
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> rock_params: vec4<f32>;
"#;

    #[test]
    fn a_lane_can_be_named_from_a_comment() {
        let u = material_uniform(ANNOTATED).expect("the binding is a block");
        let f = &u.fields[0];
        assert_eq!(f.hints.len(), 4, "one per lane: {:?}", f.hints);
        assert_eq!(f.hints[0].label, "grain_freq");
        assert_eq!(f.hints[0].max, Some(8.0));
        assert_eq!(f.hints[2].label, "bump");
        assert_eq!(f.hints[3].default, Some(0.22));
    }

    #[test]
    fn an_unannotated_material_carries_no_hints() {
        let u = material_uniform(SPIRAL).unwrap();
        assert!(u.fields.iter().all(|f| f.hints.is_empty()), "the convention costs nothing");
    }

    #[test]
    fn an_extension_does_not_own_its_bind_group() {
        assert!(!material_bind_group(MOLE).unwrap().owns_group());
    }

    const SPIRAL: &str = r#"
struct SpiralHoverParams {
    sand_color: vec4<f32>,
    dark_color: vec4<f32>,
    spiral_speed: f32,
    spiral_arms: f32,
    spiral_density: f32,
    ridge_sharpness: f32,
    grain_scale: f32,
    grain_strength: f32,
    edge_softness: f32,
    corner_radius: f32,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> params: SpiralHoverParams;
"#;

    #[test]
    fn a_shaders_own_parameter_block_is_described() {
        let u = material_uniform(SPIRAL).expect("the block is right there");
        assert_eq!(u.struct_name, "SpiralHoverParams");
        assert_eq!(u.variable, "params");
        assert_eq!(u.binding, 0);
        assert_eq!(u.fields.len(), 10);
        assert_eq!(u.size, 64);
    }

    #[test]
    fn offsets_follow_the_alignment_rules() {
        let u = material_uniform(SPIRAL).unwrap();
        let at = |n: &str| u.fields.iter().find(|f| f.name == n).unwrap().offset;
        assert_eq!(at("sand_color"), 0);
        assert_eq!(at("dark_color"), 16);
        assert_eq!(at("spiral_speed"), 32);
        assert_eq!(at("spiral_arms"), 36);
        assert_eq!(at("corner_radius"), 60);
    }

    #[test]
    fn a_vec3_aligns_to_sixteen_and_leaves_a_hole() {
        let src = r#"
            struct P { a: f32, b: vec3<f32>, c: f32, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
        "#;
        let u = material_uniform(src).unwrap();
        assert_eq!(u.fields[0].offset, 0);
        assert_eq!(u.fields[1].offset, 16);
        // `c` follows the vec3's three components, in the fourth slot it did not use.
        assert_eq!(u.fields[2].offset, 28);
        assert_eq!(u.size, 32);
    }

    #[test]
    fn a_mat3x3_is_forty_eight_bytes_not_thirty_six() {
        // Three columns of `vec3`, each padded out to 16. A packer that multiplies 3×3×4 puts
        // every later field 12 bytes early and nothing looks broken until the picture is wrong.
        let src = r#"
            struct P { m: mat3x3<f32>, tail: f32, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
        "#;
        let u = material_uniform(src).unwrap();
        let m = &u.fields[0];
        assert_eq!((m.columns, m.rows), (3, 3));
        assert_eq!(m.column_stride, 16);
        assert_eq!(m.size, 48);
        assert!(!m.is_simple());
        assert_eq!(u.fields[1].offset, 48);
    }

    #[test]
    fn a_mat4x4_is_sixty_four_bytes() {
        let src = r#"
            struct P { m: mat4x4<f32>, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
        "#;
        let u = material_uniform(src).unwrap();
        assert_eq!(u.fields[0].size, 64);
        assert_eq!(u.size, 64);
    }

    #[test]
    fn textures_and_samplers_are_reported_beside_the_block() {
        // The half that was missing: a panel that only learns about the uniform builds a form
        // omitting what the material also needs, and the pipeline will not run without it.
        let src = r#"
            struct P { tint: vec4<f32>, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
            @group(#{MATERIAL_BIND_GROUP}) @binding(1) var base: texture_2d<f32>;
            @group(#{MATERIAL_BIND_GROUP}) @binding(2) var base_sampler: sampler;
            @group(#{MATERIAL_BIND_GROUP}) @binding(3) var sky: texture_cube<f32>;
        "#;
        let g = material_bind_group(src).unwrap();
        assert_eq!(g.uniform().unwrap().struct_name, "P");
        assert_eq!(g.resources.len(), 3);
        assert_eq!(g.resources[0].kind, ResourceKind::Texture);
        assert_eq!(g.resources[0].name, "base");
        assert_eq!(g.resources[1].kind, ResourceKind::Sampler);
        assert_eq!(g.resources[2].kind, ResourceKind::Texture);
    }

    #[test]
    fn a_material_can_be_textures_only() {
        // No parameter block at all is a perfectly ordinary material, and answering `None`
        // for the whole group would hide the two bindings it does have.
        let src = r#"
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var tex: texture_2d<f32>;
            @group(#{MATERIAL_BIND_GROUP}) @binding(1) var samp: sampler;
        "#;
        let g = material_bind_group(src).unwrap();
        assert!(g.uniform().is_none());
        assert_eq!(g.resources.len(), 2);
    }

    #[test]
    fn a_storage_texture_is_not_a_sampled_one() {
        let src = r#"
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var out_tex: texture_storage_2d<rgba8unorm, write>;
        "#;
        let g = material_bind_group(src).unwrap();
        assert_eq!(g.resources[0].kind, ResourceKind::StorageTexture);
    }

    #[test]
    fn the_views_uniform_is_not_the_materials() {
        // `@group(0)` is the view in every Bevy version there has been.
        let src = r#"
            struct View { clip_from_world: mat4x4<f32>, };
            @group(0) @binding(0) var<uniform> view: View;
        "#;
        assert!(material_bind_group(src).is_none());
    }

    #[test]
    fn a_commented_out_binding_is_not_one() {
        // WGSL block comments nest, and `blank_comments` is why this holds.
        let src = r#"
            /* /* not this one */ @group(#{MATERIAL_BIND_GROUP}) @binding(0)
               var<uniform> p: Gone; */
            struct Real { a: f32, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> real: Real;
        "#;
        let u = material_uniform(src).unwrap();
        assert_eq!(u.struct_name, "Real");
    }

    #[test]
    fn an_undescribable_member_refuses_the_block_but_keeps_the_resources() {
        // Half a struct is worse than none: every offset after the unknown member would be a
        // guess. The texture beside it is still a fact, and still has to be supplied — so the
        // binding that could not be described is reported as a resource rather than dropped.
        let src = r#"
            struct P { a: f32, weird: array<vec4<f32>, 4>, };
            @group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: P;
            @group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
        "#;
        let g = material_bind_group(src).unwrap();
        assert!(g.uniform().is_none());
        assert_eq!(g.resources.len(), 2);
        assert!(g.resources.iter().any(|r| r.name == "p"));
        assert!(g.resources.iter().any(|r| r.name == "tex"));
    }

    #[test]
    fn a_shader_with_no_material_group_says_so() {
        assert!(material_bind_group("fn main() {}").is_none());
    }
}
