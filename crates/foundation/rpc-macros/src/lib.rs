//! `#[handler("method.name")]` — the proc-macro behind `arbor-rpc`.
//!
//! Annotating a backend handler turns it into a self-registering RPC entry: the
//! macro reads the function signature and generates the JSON-argument decode,
//! the result serialization, and an `inventory::submit!` so the handler shows up
//! in `arbor_rpc::registry()` with no central list. The same trick
//! `#[tauri::command]` uses, retargeted at the Model-D generic dispatch.
//!
//! Reach it through `arbor_rpc::handler` (this crate is re-exported there).

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, Pat, Type};

/// Register a function as an RPC handler.
///
/// The method name is **optional**: `#[handler]` registers under the function's
/// own name; `#[handler("custom.name")]` overrides it. Keep handler functions
/// named after their endpoint and you never repeat the string.
///
/// Shape expected: `fn(&Ctx, arg1: T1, arg2: T2, …) -> Result<R, E>` where
/// the **first parameter is the backend context** (a shared reference, e.g.
/// `&AppState`), `R: Serialize`, and `E: Display`. Each remaining argument is
/// decoded by name from the JSON params object. The context is recovered by
/// downcasting the type-erased `&dyn Any` the dispatcher passes in.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let fn_name = func.sig.ident.clone();

    // Optional name argument; defaults to the function's own name.
    let name: String = if attr.is_empty() {
        fn_name.to_string()
    } else {
        match syn::parse::<LitStr>(attr) {
            Ok(lit) => lit.value(),
            Err(e) => return e.to_compile_error().into(),
        }
    };

    let mut inputs = func.sig.inputs.iter();

    // First param = the backend context, taken by shared reference.
    let (ctx_pat, ctx_ty) = match inputs.next() {
        Some(FnArg::Typed(pt)) => match &*pt.ty {
            Type::Reference(r) => (pt.pat.clone(), r.elem.clone()),
            other => {
                return syn::Error::new_spanned(
                    other,
                    "first handler arg must be a shared reference context, e.g. `&AppState`",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                &func.sig,
                "handler needs a context first arg (e.g. `state: &AppState`)",
            )
            .to_compile_error()
            .into();
        }
    };

    // Remaining params = JSON-decodable arguments.
    let mut decode_stmts = Vec::new();
    let mut call_args = Vec::new();
    for arg in inputs {
        let FnArg::Typed(pt) = arg else { continue };
        let ident = match &*pt.pat {
            Pat::Ident(pi) => pi.ident.clone(),
            other => {
                return syn::Error::new_spanned(other, "handler args must be plain identifiers")
                    .to_compile_error()
                    .into();
            }
        };
        let ty = pt.ty.clone();
        let key = ident.to_string();
        decode_stmts.push(quote! {
            let #ident: #ty = ::arbor_rpc::decode_field(&__params, #key)?;
        });
        call_args.push(ident);
    }

    quote! {
        #func

        ::arbor_rpc::inventory::submit! {
            ::arbor_rpc::Entry {
                name: #name,
                call: |__ctx: &(dyn ::core::any::Any + 'static), __params: ::serde_json::Value|
                    -> ::core::result::Result<::serde_json::Value, ::std::string::String>
                {
                    let #ctx_pat: &#ctx_ty = __ctx
                        .downcast_ref::<#ctx_ty>()
                        .ok_or_else(|| ::std::string::String::from("rpc: wrong backend context type"))?;
                    #(#decode_stmts)*
                    let __out = #fn_name(#ctx_pat, #(#call_args),*)
                        .map_err(|__e| ::std::string::ToString::to_string(&__e))?;
                    ::serde_json::to_value(__out)
                        .map_err(|__e| ::std::string::ToString::to_string(&__e))
                },
            }
        }
    }
    .into()
}
