//! Turning the parsed hints into the `impl` the companion crate registers. Pure `proc_macro2`, so the
//! expansion is tested as text.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::parse::{HintSpec, Parsed, parse_struct};

fn option_text(text: &Option<String>) -> TokenStream {
    match text {
        Some(t) => quote! { ::core::option::Option::Some(::std::string::String::from(#t)) },
        None => quote! { ::core::option::Option::None },
    }
}

/// `Hint { color: true, range: Some((a as f64, b as f64)), .. }` for one field.
fn hint_expr(spec: &HintSpec) -> TokenStream {
    let (color, date, angle, readonly) = (spec.color, spec.date, spec.angle, spec.readonly);
    let range = match &spec.range {
        Some((lo, hi)) => quote! { ::core::option::Option::Some((((#lo) as f64), ((#hi) as f64))) },
        None => quote! { ::core::option::Option::None },
    };
    let step = match &spec.step {
        Some(s) => quote! { ::core::option::Option::Some((#s) as f64) },
        None => quote! { ::core::option::Option::None },
    };
    let (label, unit, description) = (option_text(&spec.label), option_text(&spec.unit), option_text(&spec.description));
    quote! {
        ::arbor_bevy_remote::inspect::Hint {
            color: #color, date: #date, angle: #angle, readonly: #readonly,
            range: #range, step: #step, label: #label, unit: #unit, description: #description,
        }
    }
}

/// The whole `impl RemoteInspect for T`.
pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Parsed { type_description, fields } = parse_struct(input)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    // The type's own description rides along under the empty field path.
    let type_entry = type_description.map(|d| {
        let hint = hint_expr(&HintSpec { description: Some(d), ..Default::default() });
        quote! { ("", #hint) }
    });
    let entries = type_entry.into_iter().chain(fields.iter().map(|f| {
        let (path, hint) = (&f.path, hint_expr(&f.spec));
        quote! { (#path, #hint) }
    }));
    Ok(quote! {
        impl #impl_generics ::arbor_bevy_remote::inspect::RemoteInspect for #name #ty_generics #where_clause {
            fn inspect_hints() -> ::std::vec::Vec<(&'static str, ::arbor_bevy_remote::inspect::Hint)> {
                ::std::vec![ #(#entries),* ]
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn text(input: DeriveInput) -> String {
        expand(&input).unwrap().to_string().replace(' ', "")
    }

    #[test]
    fn the_impl_names_the_type_and_lists_each_hinted_field() {
        let out = text(parse_quote! {
            struct Light { #[inspect(range = 0.0..=1.0, step = 0.01)] intensity: f32, plain: u8 }
        });
        assert!(out.contains("impl::arbor_bevy_remote::inspect::RemoteInspectforLight"), "{out}");
        assert!(out.contains("(\"intensity\",::arbor_bevy_remote::inspect::Hint{"), "{out}");
        assert!(out.contains("range:::core::option::Option::Some((((0.0)asf64),((1.0)asf64)))"), "{out}");
        assert!(out.contains("step:::core::option::Option::Some((0.01)asf64)"), "{out}");
        assert!(!out.contains("\"plain\""), "{out}");
    }

    #[test]
    fn flags_and_text_are_carried() {
        let out = text(parse_quote! {
            struct S { #[inspect(color, label = "tint")] c: [f32; 3] }
        });
        assert!(out.contains("color:true") && out.contains("angle:false"), "{out}");
        assert!(out.contains("label:::core::option::Option::Some(::std::string::String::from(\"tint\"))"), "{out}");
    }

    #[test]
    fn descriptions_are_carried_for_fields_and_the_type() {
        let out = text(parse_quote! {
            /// A lamp.
            struct Lamp {
                /// Brightness.
                b: f32,
            }
        });
        assert!(out.contains("(\"\",::arbor_bevy_remote::inspect::Hint{"), "{out}");
        assert!(out.contains("description:::core::option::Option::Some(::std::string::String::from(\"Brightness.\"))"), "{out}");
        assert!(out.contains("description:::core::option::Option::Some(::std::string::String::from(\"Alamp.\"))"), "{out}");
    }

    #[test]
    fn generics_and_where_clauses_survive() {
        let out = text(parse_quote! {
            struct Wrap<T: Clone> where T: Send { #[inspect(angle)] a: f32, t: T }
        });
        assert!(out.contains("impl<T:Clone>::arbor_bevy_remote::inspect::RemoteInspectforWrap<T>whereT:Send"), "{out}");
    }

    #[test]
    fn a_struct_with_no_hints_still_gets_an_empty_impl() {
        let out = text(parse_quote! { struct Plain { a: u8 } });
        assert!(out.contains("::std::vec![]") || out.contains("::std::vec!["), "{out}");
    }

    #[test]
    fn errors_pass_through() {
        assert!(expand(&parse_quote! { enum E { A } }).is_err());
    }
}
