//! WGSL's own vocabulary — what completion offers before it has read a line of your file.
//!
//! A closed list, and it can be one: WGSL is a small language with a fixed standard
//! library, so unlike a general-purpose language there is no import graph to consult and no
//! version skew to track. That is what makes offering it worthwhile with no server running.
//!
//! The lists are deliberately the ones a shader is actually written with. `textureSample`
//! and `mix` earn their place; the exhaustive set of `subgroup*` intrinsics does not, and
//! padding the list with entries that never match makes every other completion slower to
//! read.

/// One offered completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    pub name: &'static str,
    /// What it is, for the completion list's right-hand column.
    pub detail: &'static str,
}

const fn b(name: &'static str, detail: &'static str) -> Builtin {
    Builtin { name, detail }
}

/// The reserved words. Includes the attribute names without their `@`, because that is how
/// they are typed and a completion list that omitted them would be missing the half of a
/// shader that is annotation.
pub const KEYWORDS: &[Builtin] = &[
    b("fn", "function"),
    b("let", "immutable binding"),
    b("var", "variable"),
    b("const", "module constant"),
    b("override", "pipeline-overridable constant"),
    b("struct", "structure"),
    b("alias", "type alias"),
    b("return", "return"),
    b("if", "if"),
    b("else", "else"),
    b("loop", "loop"),
    b("for", "for"),
    b("while", "while"),
    b("break", "break"),
    b("continue", "continue"),
    b("continuing", "continuing block"),
    b("switch", "switch"),
    b("case", "case"),
    b("default", "default"),
    b("discard", "discard the fragment"),
    b("enable", "enable an extension"),
    b("requires", "require a language feature"),
    b("diagnostic", "diagnostic filter"),
    b("true", "bool"),
    b("false", "bool"),
];

/// Attributes, spelled as they are written.
pub const ATTRIBUTES: &[Builtin] = &[
    b("@vertex", "vertex entry point"),
    b("@fragment", "fragment entry point"),
    b("@compute", "compute entry point"),
    b("@workgroup_size", "compute workgroup dimensions"),
    b("@group", "bind group index"),
    b("@binding", "binding index"),
    b("@location", "IO location"),
    b("@builtin", "built-in IO value"),
    b("@interpolate", "interpolation qualifier"),
    b("@invariant", "invariant position"),
    b("@align", "member alignment"),
    b("@size", "member size"),
    b("@must_use", "result must be used"),
];

/// The type vocabulary.
pub const BUILTIN_TYPES: &[Builtin] = &[
    b("bool", "type"),
    b("i32", "32-bit signed integer"),
    b("u32", "32-bit unsigned integer"),
    b("f32", "32-bit float"),
    b("f16", "16-bit float (requires `enable f16`)"),
    b("vec2", "2-component vector"),
    b("vec3", "3-component vector"),
    b("vec4", "4-component vector"),
    b("vec2f", "vec2<f32>"),
    b("vec3f", "vec3<f32>"),
    b("vec4f", "vec4<f32>"),
    b("vec2u", "vec2<u32>"),
    b("vec3u", "vec3<u32>"),
    b("vec4u", "vec4<u32>"),
    b("vec2i", "vec2<i32>"),
    b("vec3i", "vec3<i32>"),
    b("vec4i", "vec4<i32>"),
    b("mat2x2", "matrix"),
    b("mat3x3", "matrix"),
    b("mat4x4", "matrix"),
    b("mat2x3", "matrix"),
    b("mat3x2", "matrix"),
    b("mat3x4", "matrix"),
    b("mat4x3", "matrix"),
    b("array", "array"),
    b("atomic", "atomic"),
    b("ptr", "pointer"),
    b("sampler", "sampler"),
    b("sampler_comparison", "comparison sampler"),
    b("texture_1d", "texture"),
    b("texture_2d", "texture"),
    b("texture_2d_array", "texture"),
    b("texture_3d", "texture"),
    b("texture_cube", "texture"),
    b("texture_cube_array", "texture"),
    b("texture_multisampled_2d", "texture"),
    b("texture_depth_2d", "depth texture"),
    b("texture_depth_2d_array", "depth texture"),
    b("texture_depth_cube", "depth texture"),
    b("texture_depth_cube_array", "depth texture"),
    b("texture_depth_multisampled_2d", "depth texture"),
    b("texture_storage_1d", "storage texture"),
    b("texture_storage_2d", "storage texture"),
    b("texture_storage_2d_array", "storage texture"),
    b("texture_storage_3d", "storage texture"),
];

/// Address spaces and access modes — what goes inside `var<…>`.
pub const BUILTIN_VALUES: &[Builtin] = &[
    b("function", "address space"),
    b("private", "address space"),
    b("workgroup", "address space"),
    b("uniform", "address space"),
    b("storage", "address space"),
    b("read", "access mode"),
    b("write", "access mode"),
    b("read_write", "access mode"),
    b("position", "@builtin"),
    b("vertex_index", "@builtin"),
    b("instance_index", "@builtin"),
    b("front_facing", "@builtin"),
    b("frag_depth", "@builtin"),
    b("local_invocation_id", "@builtin"),
    b("local_invocation_index", "@builtin"),
    b("global_invocation_id", "@builtin"),
    b("workgroup_id", "@builtin"),
    b("num_workgroups", "@builtin"),
    b("sample_index", "@builtin"),
    b("sample_mask", "@builtin"),
];

/// The standard library.
pub const BUILTIN_FUNCTIONS: &[Builtin] = &[
    // Arithmetic and common maths.
    b("abs", "|x|"),
    b("acos", "arc cosine"),
    b("asin", "arc sine"),
    b("atan", "arc tangent"),
    b("atan2", "arc tangent of y/x"),
    b("ceil", "round up"),
    b("clamp", "clamp(x, low, high)"),
    b("cos", "cosine"),
    b("cosh", "hyperbolic cosine"),
    b("cross", "cross product"),
    b("degrees", "radians → degrees"),
    b("determinant", "matrix determinant"),
    b("distance", "distance between points"),
    b("dot", "dot product"),
    b("exp", "e^x"),
    b("exp2", "2^x"),
    b("faceForward", "orient a vector against an incident"),
    b("floor", "round down"),
    b("fma", "fused multiply-add"),
    b("fract", "fractional part"),
    b("inverseSqrt", "1/sqrt(x)"),
    b("length", "vector length"),
    b("log", "natural log"),
    b("log2", "log base 2"),
    b("max", "maximum"),
    b("min", "minimum"),
    b("mix", "linear blend"),
    b("modf", "split into fraction and whole"),
    b("normalize", "unit vector"),
    b("pow", "x^y"),
    b("radians", "degrees → radians"),
    b("reflect", "reflect around a normal"),
    b("refract", "refract through a surface"),
    b("round", "round to nearest even"),
    b("saturate", "clamp to [0, 1]"),
    b("sign", "sign of x"),
    b("sin", "sine"),
    b("sinh", "hyperbolic sine"),
    b("smoothstep", "Hermite interpolation"),
    b("sqrt", "square root"),
    b("step", "0 or 1 at an edge"),
    b("tan", "tangent"),
    b("tanh", "hyperbolic tangent"),
    b("transpose", "transpose a matrix"),
    b("trunc", "truncate toward zero"),
    b("select", "select(f, t, cond)"),
    // Bit twiddling and packing.
    b("countLeadingZeros", "bit count"),
    b("countOneBits", "population count"),
    b("countTrailingZeros", "bit count"),
    b("extractBits", "extract a bit field"),
    b("insertBits", "insert a bit field"),
    b("reverseBits", "reverse the bits"),
    b("firstLeadingBit", "highest set bit"),
    b("firstTrailingBit", "lowest set bit"),
    b("pack4x8snorm", "pack"),
    b("pack4x8unorm", "pack"),
    b("pack2x16float", "pack"),
    b("unpack4x8snorm", "unpack"),
    b("unpack4x8unorm", "unpack"),
    b("unpack2x16float", "unpack"),
    b("bitcast", "reinterpret the bits"),
    // Derivatives — fragment stage only.
    b("dpdx", "derivative in x"),
    b("dpdy", "derivative in y"),
    b("fwidth", "|dpdx| + |dpdy|"),
    // Textures.
    b("textureDimensions", "size of a texture"),
    b("textureNumLayers", "array layer count"),
    b("textureNumLevels", "mip level count"),
    b("textureNumSamples", "sample count"),
    b("textureLoad", "read a texel by coordinate"),
    b("textureStore", "write a texel"),
    b("textureSample", "sample with a sampler"),
    b("textureSampleLevel", "sample at an explicit mip"),
    b("textureSampleBias", "sample with a LOD bias"),
    b("textureSampleGrad", "sample with explicit gradients"),
    b("textureSampleCompare", "depth comparison sample"),
    b("textureSampleCompareLevel", "depth comparison at level 0"),
    b("textureGather", "gather four texels"),
    b("textureGatherCompare", "gather with comparison"),
    // Atomics and synchronisation.
    b("atomicLoad", "atomic read"),
    b("atomicStore", "atomic write"),
    b("atomicAdd", "atomic add"),
    b("atomicSub", "atomic subtract"),
    b("atomicMax", "atomic max"),
    b("atomicMin", "atomic min"),
    b("atomicAnd", "atomic and"),
    b("atomicOr", "atomic or"),
    b("atomicXor", "atomic xor"),
    b("atomicExchange", "atomic exchange"),
    b("atomicCompareExchangeWeak", "compare and exchange"),
    b("workgroupBarrier", "workgroup memory barrier"),
    b("storageBarrier", "storage memory barrier"),
    b("textureBarrier", "texture memory barrier"),
    b("all", "all components true"),
    b("any", "any component true"),
    b("arrayLength", "runtime array length"),
];

/// Everything the language itself offers at `prefix`, in the order a list should show it.
///
/// Ordered by **what a shader is mostly made of** rather than alphabetically across the
/// whole set: functions first, then types, then the words that go inside `var<…>` and
/// `@builtin(…)`, then the keywords. A list sorted flat would bury `textureSample` under
/// twenty type names on the letter `t`.
pub fn completions_for(prefix: &str) -> Vec<Builtin> {
    let p = prefix.to_ascii_lowercase();
    let matches = |b: &&Builtin| p.is_empty() || b.name.to_ascii_lowercase().starts_with(&p);
    BUILTIN_FUNCTIONS
        .iter()
        .filter(matches)
        .chain(BUILTIN_TYPES.iter().filter(matches))
        .chain(BUILTIN_VALUES.iter().filter(matches))
        .chain(KEYWORDS.iter().filter(matches))
        .chain(ATTRIBUTES.iter().filter(matches))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_come_before_types() {
        let hits = completions_for("te");
        let f = hits.iter().position(|b| b.name == "textureSample").unwrap();
        let t = hits.iter().position(|b| b.name == "texture_2d").unwrap();
        assert!(f < t, "a shader is written with functions more often than it names types");
    }

    #[test]
    fn an_empty_prefix_offers_everything() {
        assert!(completions_for("").len() > 150);
    }

    #[test]
    fn matching_ignores_case_but_keeps_the_real_name() {
        let hits = completions_for("TEXTURESAMPLEL");
        assert_eq!(hits.first().map(|b| b.name), Some("textureSampleLevel"));
    }
}
