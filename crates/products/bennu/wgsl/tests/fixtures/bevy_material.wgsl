// A Bevy material shader, in the shape the real ones have: composed through naga_oil,
// so it opens with a braced `#import`, and it takes its bind group from a shader def.
//
// It exists as a fixture because every part of that shape is something the scanner used
// to be given only in its toy form: a multi-line import, a `#{...}` substitution inside
// an attribute, an entry point named after its own stage.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::globals,
}

struct SpiralParams {
    sand_color: vec4<f32>,
    spiral_arms: f32,
    corner_radius: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> params: SpiralParams;

// How far the mask reaches from the centre, in UV units.
const MASK_HALF: f32 = 0.44;

// Signed distance to a rounded box (Inigo Quilez).
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Hash 2D -> 1D, "Hash without Sine" (Dave Hoskins).
fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Interpolated value noise: the smooth base the grain is built on.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i + vec2<f32>(0.0, 0.0));
    let b = hash(i + vec2<f32>(1.0, 0.0));
    return mix(a, b, u.x);
}

// Fractal Brownian motion: three octaves of value noise.
fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var freq = p;
    for (var i = 0; i < 3; i += 1) {
        value += amplitude * value_noise(freq);
        freq *= 2.0;
        amplitude *= 0.5;
    }
    return value;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let t = globals.time;
    let p = mesh.uv - vec2<f32>(0.5);
    let bed = fbm(p * 8.0);
    let mask_d = sd_round_box(p, vec2<f32>(MASK_HALF), params.corner_radius);
    let sand = mix(0.0, bed, params.spiral_arms);
    return vec4<f32>(params.sand_color.rgb * sand, 1.0 - mask_d);
}
