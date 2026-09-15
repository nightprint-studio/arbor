//! `// @preview` — what a material's parameters are *called*, and what range they live in.
//!
//! ## The problem
//!
//! A Bevy material extension binds `vec4<f32>` and packs four unrelated things into it:
//!
//! ```wgsl
//! // x: grain frequency · y: albedo splotch · z: bump · w: band frequency
//! @group(#{MATERIAL_BIND_GROUP}) @binding(100)
//! var<uniform> rock_params: vec4<f32>;
//! ```
//!
//! The author already wrote down what each lane is — in a comment, for a person. A preview
//! panel cannot read that, so it offers `X Y Z W` and a range it guessed from the variable's
//! name, which for a lane that is a frequency in the tens and a lane that is an amount in
//! `[0,1]` cannot be right for both.
//!
//! ## Why a comment and not an attribute
//!
//! Because WGSL has no room for one. `naga` fails with `unknown attribute` on anything outside
//! the spec's list, so `@range(0, 8)` would stop the shader compiling — in the game as well as
//! in the preview. A comment is invisible to the compiler by construction, and this crate
//! already reads comments carefully enough to skip them.
//!
//! ## The form
//!
//! ```text
//! // @preview <label> [<min>..<max>] [= <default>] [: <hint>]
//! ```
//!
//! One line per lane, in declaration order, on the lines immediately above the declaration —
//! attribute lines in between are stepped over, because `@group`/`@binding` always sit there.
//! Everything after the label is optional: a lane can be named without being bounded, which is
//! the common case and the one worth making cheap.
//!
//! Nothing here is required of a shader. A material with no annotations is described exactly as
//! it was before, and the panel goes on guessing.

/// What one `@preview` line says about one lane.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PreviewHint {
    /// The name to show instead of `X`/`Y`/`Z`/`W`.
    pub label: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// The value to open on, rather than one derived from the range.
    pub default: Option<f32>,
    /// A sentence for the control's tooltip.
    pub hint: Option<String>,
    /// `#rrggbb` or `#rrggbbaa` — this member is a **colour**, and this is where it starts.
    ///
    /// Two things a panel cannot work out on its own. Whether a `vec4` is a colour is guessed
    /// from the variable's name, which works for `sand_color` and fails for `hot`, `deep` and
    /// `foam` — all of which are colours and none of which say so. And even when the guess is
    /// right, the STARTING colour comes from a palette, so a material opens on an arbitrary hue
    /// rather than the one its author chose.
    ///
    /// Written as a hex string because that is what an author reads and what a colour picker
    /// speaks. The conversion to linear belongs to whoever renders it — sRGB is a property of
    /// the notation, not of the value.
    pub hex: Option<String>,
}

const DIRECTIVE: &str = "@preview";

/// Parse one directive body — everything after `@preview`.
fn parse_body(body: &str) -> Option<PreviewHint> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    // The hint is split off first: it is free text and may contain anything, including the
    // characters the rest of the grammar uses.
    let (head, hint) = match body.split_once(':') {
        Some((h, t)) => (h.trim(), Some(t.trim().to_string()).filter(|s| !s.is_empty())),
        None => (body, None),
    };

    // The default may be a number or a colour, and the `#` tells them apart before anything
    // tries to parse one as the other.
    let mut hex = None;
    let (head, default) = match head.split_once('=') {
        Some((h, d)) => {
            let d = d.trim();
            if let Some(rest) = d.strip_prefix('#') {
                let ok = matches!(rest.len(), 6 | 8) && rest.chars().all(|c| c.is_ascii_hexdigit());
                if ok {
                    hex = Some(format!("#{}", rest.to_ascii_lowercase()));
                }
                (h.trim(), None)
            } else {
                (h.trim(), d.parse::<f32>().ok())
            }
        }
        None => (head, None),
    };

    // What is left is `<label>` or `<label> <min>..<max>`. The label is the first word; the
    // range, if there is one, is the last — so a label may contain spaces without ambiguity.
    let mut min = None;
    let mut max = None;
    let mut label = head.to_string();

    if let Some((before, range)) = head.rsplit_once(' ') {
        if let Some((lo, hi)) = range.split_once("..") {
            if let (Ok(lo), Ok(hi)) = (lo.trim().parse::<f32>(), hi.trim().parse::<f32>()) {
                min = Some(lo);
                max = Some(hi);
                label = before.trim().to_string();
            }
        }
    } else if let Some((lo, hi)) = head.split_once("..") {
        // A range with no label: legal, and means "leave the name alone, fix the bounds".
        if let (Ok(lo), Ok(hi)) = (lo.trim().parse::<f32>(), hi.trim().parse::<f32>()) {
            min = Some(lo);
            max = Some(hi);
            label = String::new();
        }
    }

    if label.is_empty() && min.is_none() && default.is_none() && hint.is_none() && hex.is_none() {
        return None;
    }
    Some(PreviewHint { label, min, max, default, hint, hex })
}

/// The directives attached to the declaration starting at `offset`, in source order.
///
/// Walks backwards from the declaration's line: attribute lines are stepped over (`@group` and
/// `@binding` are always between the comment and the `var`), comment lines are collected, and
/// anything else ends the run. A blank line ends it too — a comment separated from what it
/// describes is describing something else.
pub fn hints_before(source: &str, offset: usize) -> Vec<PreviewHint> {
    let head = &source[..offset.min(source.len())];
    // Every line before the one the declaration is on.
    let mut lines: Vec<&str> = head.lines().collect();
    lines.pop();

    let mut collected: Vec<PreviewHint> = Vec::new();
    for line in lines.iter().rev() {
        let t = line.trim();
        if t.is_empty() {
            break;
        }
        if t.starts_with('@') {
            // `@group(...) @binding(...)` on its own line above the `var`, which is why
            // attribute lines are stepped over at all.
            //
            // But only when the line is JUST attributes. A shader that writes the whole
            // declaration on one line — `@group(3) @binding(100) var top_albedo: …;`, which is
            // how `tile.wgsl` writes all ten of its textures — puts a line starting with `@`
            // above the next declaration too. Stepping over that one walks into the PREVIOUS
            // binding's comment, and every texture in the file ends up wearing the annotation
            // written for the first.
            if t.contains("var") || t.ends_with(';') {
                break;
            }
            continue;
        }
        let Some(rest) = t.strip_prefix("//") else { break };
        // `///` and `//!` are comments too; the directive is what matters, not the flavour.
        let rest = rest.trim_start_matches(['/', '!']).trim();
        let Some(body) = rest.strip_prefix(DIRECTIVE) else {
            // A plain comment above the directives is prose, and stops the run only if it
            // sits BETWEEN them — which reversed iteration turns into "keep going".
            continue;
        };
        if let Some(h) = parse_body(body) {
            collected.push(h);
        }
    }
    // Collected bottom-up; the lanes are named top-down.
    collected.reverse();
    collected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_label_is_enough() {
        let h = parse_body(" grain_freq").expect("a name on its own is the common case");
        assert_eq!(h.label, "grain_freq");
        assert_eq!(h.min, None);
        assert_eq!(h.default, None);
    }

    #[test]
    fn a_range_and_a_default_are_read() {
        let h = parse_body("grain_freq 0.2..8 = 1.6").unwrap();
        assert_eq!(h.label, "grain_freq");
        assert_eq!(h.min, Some(0.2));
        assert_eq!(h.max, Some(8.0));
        assert_eq!(h.default, Some(1.6));
    }

    #[test]
    fn a_hex_default_marks_the_lane_as_a_colour() {
        let h = parse_body("hot = #ff6b14 : the hottest part of a crack").unwrap();
        assert_eq!(h.label, "hot");
        assert_eq!(h.hex.as_deref(), Some("#ff6b14"));
        // Not also a number: `#ff6b14` is not one, and pretending otherwise would put a
        // slider next to a colour.
        assert_eq!(h.default, None);
        assert!(h.hint.unwrap().starts_with("the hottest"));
    }

    #[test]
    fn an_eight_digit_hex_carries_alpha_and_a_bad_one_is_ignored() {
        assert_eq!(parse_body("tint = #11223344").unwrap().hex.as_deref(), Some("#11223344"));
        // Wrong length or a non-hex digit is not a colour. The LANE survives — it still has a
        // name, and throwing that away because the default was mistyped would lose more than
        // it fixes — but it does not quietly become a colour, which would put the panel's idea
        // of the material out of step with the shader's.
        assert_eq!(parse_body("tint = #12345").unwrap().hex, None);
        assert_eq!(parse_body("tint = #gggggg").unwrap().hex, None);
    }

    #[test]
    fn a_negative_bound_survives_the_split() {
        let h = parse_body("warp -1..1").unwrap();
        assert_eq!(h.min, Some(-1.0));
        assert_eq!(h.max, Some(1.0));
    }

    #[test]
    fn a_hint_may_contain_the_grammar_s_own_characters() {
        let h = parse_body("bump 0..1 = 0.65 : how far normals move, 0..1 = none..lots").unwrap();
        assert_eq!(h.label, "bump");
        assert_eq!(h.max, Some(1.0));
        assert_eq!(h.default, Some(0.65));
        assert!(h.hint.unwrap().starts_with("how far normals move"));
    }

    #[test]
    fn a_label_may_contain_spaces() {
        let h = parse_body("albedo splotch 0..1").unwrap();
        assert_eq!(h.label, "albedo splotch");
        assert_eq!(h.min, Some(0.0));
    }

    #[test]
    fn a_range_with_no_label_leaves_the_name_alone() {
        let h = parse_body("0..64").unwrap();
        assert_eq!(h.label, "");
        assert_eq!(h.max, Some(64.0));
    }

    const ROCK: &str = r#"
// The rock's noise parameters.
// @preview grain_freq 0.2..8 = 1.6
// @preview albedo_splotch 0..1 = 0.45
// @preview bump 0..1 = 0.65
// @preview band_freq 0..2 = 0.22
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> rock_params: vec4<f32>;
"#;

    #[test]
    fn directives_are_found_past_the_attribute_line() {
        let at = ROCK.find("rock_params").unwrap();
        let hints = hints_before(ROCK, at);
        assert_eq!(hints.len(), 4, "{hints:?}");
        assert_eq!(hints[0].label, "grain_freq");
        assert_eq!(hints[3].label, "band_freq");
        assert_eq!(hints[3].max, Some(2.0));
    }

    #[test]
    fn prose_between_directives_does_not_end_the_run() {
        let src = "// @preview a\n// just explaining\n// @preview b\n@binding(100)\nvar<uniform> p: vec2<f32>;";
        let at = src.find("p: vec2").unwrap();
        let hints = hints_before(src, at);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].label, "a");
        assert_eq!(hints[1].label, "b");
    }

    #[test]
    fn a_one_line_declaration_does_not_inherit_the_one_above_it() {
        // The shape `tile.wgsl` uses for every texture it binds. Before this, the second
        // declaration stepped over the first — it starts with `@` — and collected the comment
        // written for it, so a file annotating one texture annotated all of them.
        let src = concat!(
            "// @preview diffuse\n",
            "@group(3) @binding(100) var top_albedo: texture_2d<f32>;\n",
            "@group(3) @binding(101) var side_albedo: texture_2d<f32>;\n",
        );
        let first = hints_before(src, src.find("top_albedo").unwrap());
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].label, "diffuse");

        let second = hints_before(src, src.find("side_albedo").unwrap());
        assert!(second.is_empty(), "the comment belongs to the line above it: {second:?}");
    }

    #[test]
    fn a_blank_line_detaches_the_comment() {
        let src = "// @preview a\n\n@binding(100)\nvar<uniform> p: f32;";
        let at = src.find("p: f32").unwrap();
        assert!(hints_before(src, at).is_empty(), "a detached comment describes something else");
    }

    #[test]
    fn a_struct_member_carries_its_colour() {
        // La forma con cui gli esempi del pacchetto dichiarano i propri colori: la riga sta
        // sopra un MEMBRO di struct, non sopra un `var`, ed e' li' che serve — un `vec4`
        // chiamato `hot` e' un colore che nessuna euristica sul nome riconoscera' mai.
        let src = concat!(
            "struct MagmaParams {\n",
            "    // @preview hot = #ff6b14 : The centre of a crack.\n",
            "    hot: vec4<f32>,\n",
            "};\n",
        );
        let at = src.find("hot: vec4").unwrap();
        let hints = hints_before(src, at);
        assert_eq!(hints.len(), 1, "{hints:?}");
        assert_eq!(hints[0].hex.as_deref(), Some("#ff6b14"));
        assert_eq!(hints[0].label, "hot");
    }

    #[test]
    fn an_unannotated_declaration_has_none() {
        let src = "// ordinary prose\n@binding(100)\nvar<uniform> p: f32;";
        let at = src.find("p: f32").unwrap();
        assert!(hints_before(src, at).is_empty());
    }
}
