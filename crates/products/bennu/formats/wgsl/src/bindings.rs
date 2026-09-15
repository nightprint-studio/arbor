//! The resources a shader declares: `@group(g) @binding(b) var<…> name: T;`
//!
//! Its own module rather than a field on [`crate::symbols::WgslSymbol`], because a binding is a
//! different kind of fact from a declaration. A symbol answers *what is written here*; a binding
//! answers *what the pipeline must supply*, and that is the half a Bevy material's
//! `#[derive(AsBindGroup)]` has to agree with. The two are read from the same attributes and go
//! to different readers.
//!
//! ## The group is text, not a number
//!
//! In a Bevy shader the material's group is written `@group(#{MATERIAL_BIND_GROUP})` — a
//! naga_oil shader def substituted before the compiler ever sees it. There is no number to
//! return, and inventing one (2, which is what it usually is) would be a guess that is silently
//! wrong on an extension material. So the group is kept **verbatim** and callers decide what
//! they can conclude from it: [`Binding::in_material_group`] answers the only question anybody
//! actually asks, and answers it conservatively.

/// One `@group @binding` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The group expression as written — `"2"`, `"#{MATERIAL_BIND_GROUP}"`, `"0"`.
    pub group: String,
    /// The binding index. A binding whose index is not a literal is not reported at all: there
    /// is nothing to match it against.
    pub index: u32,
    /// The variable's name.
    pub name: String,
    /// The declared type, verbatim (`SpiralHoverParams`, `texture_2d<f32>`, `sampler`).
    pub ty: String,
    /// What is inside `var<…>`, verbatim and without the angle brackets — `"uniform"`,
    /// `"storage, read"`, or empty for a texture or a sampler, which have none.
    ///
    /// Kept because the type alone cannot tell a uniform block from a storage buffer: both are
    /// a struct name, and only the address space says which. A previewer that gets that wrong
    /// hands a uniform buffer to a binding declared `var<storage>` and the pipeline is refused.
    pub address_space: String,
    /// Byte offsets of the NAME, so a jump lands on it.
    pub start: usize,
    pub end: usize,
    /// Byte span of the INDEX inside `@binding(…)`, so it can be rewritten in place.
    ///
    /// Recorded because a preview cannot ask wgpu for a layout that matches whatever indices a
    /// shader happens to use — `AsBindGroup::bind_group_layout_entries` is a static method, so
    /// one material type has exactly one layout. What it CAN do is renumber the shader onto a
    /// layout it already has, which needs to know where each number is written. See
    /// [`crate::preview_layout`].
    pub index_start: usize,
    pub index_end: usize,
}

impl Binding {
    /// Whether this binding could belong to the **material's** bind group.
    ///
    /// Two forms, and only one of them is certain.
    ///
    /// `@group(#{MATERIAL_BIND_GROUP})` is the material's by construction — naga_oil substitutes
    /// whatever the number happens to be, which is the entire reason the def exists.
    ///
    /// A **literal** group cannot be adjudicated from here. Which number the material's group is
    /// depends on the Bevy version and on the kind of material: it has been 1, it has been 2, and
    /// a `MaterialExtension` in Bevy 0.18 writes 3. Pinning one of those would report every
    /// correctly-bound shader written against a different release as broken. What *is* stable is
    /// the other end: **group 0 is the view** and group 1 is the mesh and lights, in every version
    /// there has been. So a literal 2 or above is accepted and a literal 0 or 1 is not — which
    /// keeps a material's uniform at binding 0 from matching the view's uniform at binding 0,
    /// while never claiming a material is missing a binding it plainly declares.
    pub fn in_material_group(&self) -> bool {
        let g = self.group.trim();
        if g.contains("MATERIAL_BIND_GROUP") || g.contains("#{") {
            return true;
        }
        g.parse::<u32>().is_ok_and(|n| n >= 2)
    }
}

/// Every binding the shader declares, in source order.
///
/// Reads the same masked text the symbol scan does, so a `@binding(0)` inside a comment is not
/// one. Tolerant by construction, like the rest of this crate: a malformed attribute is skipped
/// rather than fatal.
pub fn scan(source: &str) -> Vec<Binding> {
    let blanked = crate::symbols::blank_comments(source);
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // Anchored on the `var`, not on the attribute: the attributes may be on their own line,
        // in either order, and it is the declaration that has the name and the type.
        if !word_at(bytes, i, b"var") {
            i += 1;
            continue;
        }
        let (attrs, attrs_at) = attributes_before(&blanked, i);
        let after_var = i + 3;
        // `var<uniform>` — the address space, skipped over to reach the name.
        let mut j = after_var;
        let mut address_space = String::new();
        if bytes.get(j) == Some(&b'<') {
            match blanked[j..].find('>') {
                Some(k) => {
                    address_space = source[j + 1..j + k].trim().to_string();
                    j += k + 1;
                }
                None => {
                    i = after_var;
                    continue;
                }
            }
        }
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        let start = j;
        while j < bytes.len() && is_ident_byte(bytes[j]) {
            j += 1;
        }
        if j == start {
            i = after_var;
            continue;
        }
        let name = source[start..j].to_string();
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        let ty = match bytes.get(j) {
            Some(&b':') => type_until_semicolon(source, j + 1),
            _ => String::new(),
        };
        if let (Some((group, _)), Some((index, span))) =
            (arg_of(&attrs, "@group("), arg_of(&attrs, "@binding("))
        {
            if let Ok(parsed) = index.trim().parse::<u32>() {
                // The argument may be padded — `@binding( 100 )` — so the span is narrowed to
                // the number itself before being made absolute. A rewrite that replaced the
                // padding too would still compile; keeping it means a renumbered shader reads
                // like the one that was written.
                let lead = index.len() - index.trim_start().len();
                let index_start = attrs_at + span.0 + lead;
                let index_end = index_start + index.trim().len();
                out.push(Binding {
                    group,
                    index: parsed,
                    name,
                    ty,
                    address_space,
                    start,
                    end: j.min(source.len()),
                    index_start,
                    index_end,
                });
            }
        }
        i = after_var;
    }
    out
}

/// Everything from after the `:` to the `;` that ends the declaration.
fn type_until_semicolon(src: &str, from: usize) -> String {
    let bytes = src.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b';' if depth <= 0 => break,
            _ => {}
        }
        i += 1;
    }
    src[from..i.min(src.len())].trim().to_string()
}

/// The attributes immediately before `at`, and the offset they start at.
///
/// The same walk-backwards the symbol scan uses, kept here so this module reads on its own.
/// The offset comes back too so a span inside the slice can be made absolute — which is what
/// turns "there is a `@binding(104)` here" into "the `104` is at byte 2117".
fn attributes_before(src: &str, at: usize) -> (String, usize) {
    let bytes = src.as_bytes();
    let mut start = at;
    let mut depth = 0usize;
    while start > 0 {
        let b = bytes[start - 1];
        if b == b')' {
            depth += 1;
        } else if b == b'(' {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else if depth == 0
            && !(b as char).is_whitespace()
            && !is_ident_byte(b)
            && b != b'@'
            && b != b','
            && b != b'#'
            && b != b'{'
            && b != b'}'
        {
            break;
        }
        start -= 1;
    }
    (src[start..at].to_string(), start)
}

/// The argument of the first `open(` in `attrs`, verbatim, and its span within `attrs`.
fn arg_of(attrs: &str, open: &str) -> Option<(String, (usize, usize))> {
    let lower = attrs.to_ascii_lowercase();
    let at = lower.find(&open.to_ascii_lowercase())? + open.len();
    // Balanced, because `@group(#{MATERIAL_BIND_GROUP})` has a brace group inside it.
    let bytes = attrs.as_bytes();
    let mut depth = 0i32;
    let mut i = at;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'{' => depth += 1,
            b'}' => depth -= 1,
            b')' if depth == 0 => return Some((attrs[at..i].to_string(), (at, i))),
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn word_at(bytes: &[u8], at: usize, word: &[u8]) -> bool {
    if !bytes[at..].starts_with(word) {
        return false;
    }
    let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
    let after = at + word.len();
    let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHADER: &str = concat!(
        "#import bevy_pbr::forward_io::VertexOutput\n\n",
        "struct SpiralHoverParams {\n",
        "    sand_color: vec4<f32>,\n",
        "    spiral_speed: f32,\n",
        "};\n\n",
        "@group(#{MATERIAL_BIND_GROUP}) @binding(0)\n",
        "var<uniform> params: SpiralHoverParams;\n\n",
        "@group(#{MATERIAL_BIND_GROUP}) @binding(1)\n",
        "var base_texture: texture_2d<f32>;\n",
        "@group(#{MATERIAL_BIND_GROUP}) @binding(2)\n",
        "var base_sampler: sampler;\n\n",
        "@group(0) @binding(0) var<uniform> view: View;\n",
        "// @group(2) @binding(9) var ghost: sampler;\n",
    );

    #[test]
    fn every_binding_is_found_with_its_name_and_type() {
        let all = scan(SHADER);
        let names: Vec<&str> = all.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["params", "base_texture", "base_sampler", "view"]);
        assert_eq!(all[0].ty, "SpiralHoverParams");
        assert_eq!(all[1].ty, "texture_2d<f32>");
        assert_eq!(all[2].ty, "sampler");
    }

    #[test]
    fn a_literal_group_above_the_engines_own_is_taken_as_the_materials() {
        // Bevy 0.18 writes 3 for a `MaterialExtension`; older releases wrote 2. Which number it
        // is is not something this side can decide, and rejecting the ones it did not expect
        // would report a correct shader as broken.
        let src = concat!(
            "@group(3) @binding(100) var top: texture_2d<f32>;\n",
            "@group(1) @binding(0) var<uniform> mesh: Mesh;\n",
        );
        let all = scan(src);
        assert!(all[0].in_material_group(), "group 3 is the material's here");
        assert!(!all[1].in_material_group(), "group 1 is the engine's, in every version");
    }

    #[test]
    fn a_shader_def_group_is_kept_as_written() {
        // The one thing that must not be guessed. `#{MATERIAL_BIND_GROUP}` is substituted by
        // naga_oil, and turning it into `2` here would be silently wrong on an extension
        // material — where the material group is not 2.
        let all = scan(SHADER);
        assert_eq!(all[0].group, "#{MATERIAL_BIND_GROUP}");
        assert!(all[0].in_material_group());
        assert_eq!(all[3].group, "0");
        assert!(!all[3].in_material_group(), "the view group is not the material's");
    }

    #[test]
    fn the_name_span_points_at_the_name() {
        let all = scan(SHADER);
        assert_eq!(&SHADER[all[0].start..all[0].start + all[0].name.len()], "params");
    }

    #[test]
    fn a_commented_out_binding_is_not_one() {
        let all = scan(SHADER);
        assert!(all.iter().all(|b| b.name != "ghost"));
    }

    #[test]
    fn the_index_span_points_at_the_number_and_nothing_else() {
        let all = scan(SHADER);
        assert_eq!(&SHADER[all[1].index_start..all[1].index_end], "1");
        let padded = "@group(3) @binding( 104 )\nvar top: texture_2d<f32>;\n";
        let one = scan(padded);
        assert_eq!(&padded[one[0].index_start..one[0].index_end], "104");
    }

    #[test]
    fn a_shader_with_no_bindings_answers_empty_rather_than_guessing() {
        assert!(scan("fn main() -> f32 { return 1.0; }").is_empty());
    }
}
