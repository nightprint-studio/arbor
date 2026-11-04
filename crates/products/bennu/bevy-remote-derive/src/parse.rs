//! Reading `#[inspect(...)]` off a struct into a plain model. Nothing here emits code, so it is tested
//! on `syn::parse_quote!` input without a compiler plugin.
//!
//! Grammar, all keys optional and combinable in one attribute or spread over several:
//!
//! | Key | Meaning |
//! |---|---|
//! | `color` | the field is a colour (hex string, `[r, g, b(, a)]` or a `{red, green, blue}` struct) |
//! | `range = a..=b` | draw a slider over `a..=b` |
//! | `step = x` | the slider's / scrubber's step |
//! | `date` | seconds since the Unix epoch, drawn as a date |
//! | `angle` | radians, drawn and edited in degrees |
//! | `readonly` | shown, not editable |
//! | `label = "…"` | the name shown instead of the field's |
//! | `unit = "…"` | a suffix such as `m/s` |
//! | `description = "…"` | a sentence the inspector shows under the label and on hover; defaults to the `///` doc comment |

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{Data, DeriveInput, Error, Expr, Fields, Index, LitStr, RangeLimits, Result};

/// What one field asked for. Numbers stay as the tokens the user wrote (`-1.0`, `MAX_HP as f32`), because
/// only the compiler can evaluate them.
#[derive(Debug, Default)]
pub struct HintSpec {
    pub color: bool,
    pub date: bool,
    pub angle: bool,
    pub readonly: bool,
    pub range: Option<(TokenStream, TokenStream)>,
    pub step: Option<TokenStream>,
    pub label: Option<String>,
    pub unit: Option<String>,
    pub description: Option<String>,
}

impl HintSpec {
    /// A spec that asks for nothing is not worth sending.
    pub fn is_empty(&self) -> bool {
        !self.color
            && !self.date
            && !self.angle
            && !self.readonly
            && self.range.is_none()
            && self.step.is_none()
            && self.label.is_none()
            && self.unit.is_none()
            && self.description.is_none()
    }
}

/// A field with its dotted path (`speed`, or `0` in a tuple struct) and what it asked for.
#[derive(Debug)]
pub struct FieldHint {
    pub path: String,
    pub spec: HintSpec,
}

const KEYS: &str = "color, range, step, date, angle, readonly, label, unit, description";

fn parse_attr(spec: &mut HintSpec, attr: &syn::Attribute) -> Result<()> {
    attr.parse_nested_meta(|meta| {
        let key = meta.path.get_ident().map(ToString::to_string).unwrap_or_default();
        match key.as_str() {
            "color" => spec.color = true,
            "date" => spec.date = true,
            "angle" => spec.angle = true,
            "readonly" => spec.readonly = true,
            "range" => {
                let Expr::Range(range) = meta.value()?.parse::<Expr>()? else {
                    return Err(meta.error("`range` takes a range: `range = 0.0..=1.0`"));
                };
                let (Some(start), Some(end)) = (range.start.as_ref(), range.end.as_ref()) else {
                    return Err(meta.error("`range` needs both ends: `range = 0.0..=1.0`"));
                };
                if !matches!(range.limits, RangeLimits::Closed(_)) {
                    return Err(meta.error("write the range inclusive, `a..=b`: the slider reaches its end"));
                }
                spec.range = Some((start.to_token_stream(), end.to_token_stream()));
            }
            "step" => {
                let step: Expr = meta.value()?.parse()?;
                spec.step = Some(step.to_token_stream());
            }
            "label" => spec.label = Some(meta.value()?.parse::<LitStr>()?.value()),
            "unit" => spec.unit = Some(meta.value()?.parse::<LitStr>()?.value()),
            "description" => spec.description = Some(meta.value()?.parse::<LitStr>()?.value()),
            other => return Err(meta.error(format!("unknown inspect option `{other}`; expected one of: {KEYS}"))),
        }
        Ok(())
    })
}

/// Reject combinations that cannot be drawn: a colour is not a slider, a date is not an angle.
fn check(spec: &HintSpec, at: &impl ToTokens) -> Result<()> {
    let kinds = [spec.color, spec.date, spec.angle, spec.range.is_some()].iter().filter(|k| **k).count();
    if kinds > 1 {
        return Err(Error::new_spanned(at, "`color`, `date`, `angle` and `range` each say how to draw the field: pick one"));
    }
    Ok(())
}

/// The text of a `///` doc comment: its lines trimmed and joined with spaces, blank lines dropped. `None` when
/// there is none, so a field without docs is not given an empty description.
pub fn doc_text(attrs: &[syn::Attribute]) -> Option<String> {
    let lines: Vec<String> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => Some(s.value().trim().to_string()),
                _ => None,
            },
            _ => None,
        })
        .filter(|l| !l.is_empty())
        .collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// What a struct's derive found: a description of the type itself, and the hinted fields in field order.
#[derive(Debug)]
pub struct Parsed {
    pub type_description: Option<String>,
    pub fields: Vec<FieldHint>,
}

/// The type's own description: `#[inspect(description = "…")]` on the struct, else its doc comment. Any
/// other option at the type level is an error — there is no field to draw.
fn type_description(input: &DeriveInput) -> Result<Option<String>> {
    let mut spec = HintSpec::default();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("inspect")) {
        parse_attr(&mut spec, attr)?;
    }
    let HintSpec { description, color, date, angle, readonly, range, step, label, unit } = spec;
    if color || date || angle || readonly || range.is_some() || step.is_some() || label.is_some() || unit.is_some() {
        return Err(Error::new_spanned(&input.ident, "on a struct only `description` can be given: the other options describe how to draw a field"));
    }
    Ok(description.or_else(|| doc_text(&input.attrs)))
}

/// The hints of a struct: its description and every field that has any `#[inspect(...)]` or doc comment.
pub fn parse_struct(input: &DeriveInput) -> Result<Parsed> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(&input.ident, "`RemoteInspect` can only be derived for a struct"));
    };
    let mut out = Vec::new();
    let fields: Vec<(String, &syn::Field)> = match &data.fields {
        Fields::Named(f) => f.named.iter().map(|f| (f.ident.as_ref().map(ToString::to_string).unwrap_or_default(), f)).collect(),
        Fields::Unnamed(f) => f.unnamed.iter().enumerate().map(|(i, f)| (Index::from(i).index.to_string(), f)).collect(),
        Fields::Unit => Vec::new(),
    };
    for (path, field) in fields {
        let mut spec = HintSpec::default();
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("inspect")) {
            parse_attr(&mut spec, attr)?;
        }
        if spec.description.is_none() {
            spec.description = doc_text(&field.attrs);
        }
        check(&spec, field)?;
        if !spec.is_empty() {
            out.push(FieldHint { path, spec });
        }
    }
    Ok(Parsed { type_description: type_description(input)?, fields: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn hints(input: DeriveInput) -> Result<Vec<FieldHint>> {
        parse_struct(&input).map(|p| p.fields)
    }

    #[test]
    fn a_field_can_ask_for_several_things() {
        let found = hints(parse_quote! {
            struct Light {
                #[inspect(range = 0.0..=1.0, step = 0.01)]
                intensity: f32,
                #[inspect(color)]
                tint: [f32; 3],
                plain: u8,
            }
        })
        .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, "intensity");
        let (lo, hi) = found[0].spec.range.as_ref().unwrap();
        assert_eq!((lo.to_string(), hi.to_string()), ("0.0".into(), "1.0".into()));
        assert_eq!(found[0].spec.step.as_ref().unwrap().to_string(), "0.01");
        assert!(found[1].spec.color);
    }

    #[test]
    fn options_can_be_spread_over_attributes_and_carry_text() {
        let found = hints(parse_quote! {
            struct Body {
                #[inspect(label = "move speed")]
                #[inspect(unit = "m/s", readonly)]
                speed: f32,
            }
        })
        .unwrap();
        let spec = &found[0].spec;
        assert_eq!(spec.label.as_deref(), Some("move speed"));
        assert_eq!(spec.unit.as_deref(), Some("m/s"));
        assert!(spec.readonly);
    }

    #[test]
    fn a_range_keeps_negative_and_computed_ends_as_written() {
        let found = hints(parse_quote! { struct S { #[inspect(range = -1.0..=MAX as f64)] x: f64 } }).unwrap();
        let (lo, hi) = found[0].spec.range.as_ref().unwrap();
        assert_eq!(lo.to_string().replace(' ', ""), "-1.0");
        assert_eq!(hi.to_string(), "MAX as f64");
    }

    #[test]
    fn a_tuple_struct_is_addressed_by_index() {
        let found = hints(parse_quote! { struct Hp(#[inspect(angle)] f32, u8); }).unwrap();
        assert_eq!(found[0].path, "0");
    }

    #[test]
    fn mistakes_say_what_was_expected() {
        let unknown = hints(parse_quote! { struct S { #[inspect(colour)] x: f32 } }).unwrap_err().to_string();
        assert!(unknown.contains("unknown inspect option `colour`") && unknown.contains("color, range"), "{unknown}");
        let half_open = hints(parse_quote! { struct S { #[inspect(range = 0.0..1.0)] x: f32 } }).unwrap_err().to_string();
        assert!(half_open.contains("inclusive"), "{half_open}");
        let both = hints(parse_quote! { struct S { #[inspect(color, angle)] x: f32 } }).unwrap_err().to_string();
        assert!(both.contains("pick one"), "{both}");
        let en = hints(parse_quote! { enum E { A } }).unwrap_err().to_string();
        assert!(en.contains("only be derived for a struct"), "{en}");
    }

    #[test]
    fn a_doc_comment_is_the_default_description_and_the_attribute_wins() {
        let found = hints(parse_quote! {
            struct S {
                /// Metres per second.
                ///
                /// Capped by the engine.
                speed: f32,
                /// Ignored.
                #[inspect(description = "Hit points")]
                hp: u32,
            }
        })
        .unwrap();
        assert_eq!(found[0].spec.description.as_deref(), Some("Metres per second. Capped by the engine."));
        assert_eq!(found[1].spec.description.as_deref(), Some("Hit points"));
    }

    #[test]
    fn the_type_has_its_own_description() {
        let doc = parse_struct(&parse_quote! {
            /// A lamp.
            struct Lamp { a: u8 }
        })
        .unwrap();
        assert_eq!(doc.type_description.as_deref(), Some("A lamp."));
        let attr = parse_struct(&parse_quote! { #[inspect(description = "Light source")] struct Lamp { a: u8 } }).unwrap();
        assert_eq!(attr.type_description.as_deref(), Some("Light source"));
        let bad = parse_struct(&parse_quote! { #[inspect(color)] struct Lamp { a: u8 } }).unwrap_err().to_string();
        assert!(bad.contains("only `description`"), "{bad}");
    }

    #[test]
    fn fields_without_the_attribute_are_left_out() {
        assert!(hints(parse_quote! { struct S { a: u8, b: u8 } }).unwrap().is_empty());
        assert!(hints(parse_quote! { struct Unit; }).unwrap().is_empty());
    }
}
