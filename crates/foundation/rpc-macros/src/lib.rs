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
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Expr, ExprLit, FnArg, ItemFn, Lit, LitStr, MetaNameValue, Pat, Token, Type,
};

/// Register a function as an RPC handler.
///
/// The attribute is **optional** and accepts three forms:
/// - `#[handler]` — method name = the function's own name, default program.
/// - `#[handler("custom.name")]` — override the method name, default program.
/// - `#[handler(program = "platform", name = "custom.name")]` — set the
///   backend program (the router's product label) and, optionally, the method
///   name. Either key may be omitted; a missing `name` defaults to the
///   function's own name, a missing `program` to the default (empty) program.
///
/// Keep handler functions named after their endpoint and you never repeat the
/// method string; tag the whole module's handlers with one `program = …` each.
///
/// Shape expected: `fn(&Ctx, arg1: T1, arg2: T2, …) -> Result<R, E>` where
/// the **first parameter is the backend context** (a shared reference, e.g.
/// `&AppState`), `R: Serialize`, and `E: Display`. Each remaining argument is
/// decoded by name from the JSON params object. The context is recovered by
/// downcasting the type-erased `&dyn Any` the dispatcher passes in.
///
/// A plain `fn` registers as `Kind::Sync`; an `async fn` registers as
/// `Kind::Async` (the macro generates a thunk that downcasts the context
/// **before** the `.await` — `&dyn Any` is not `Send` — and boxes a
/// ctx-borrowing `Send` future). The host runs sync handlers on
/// `spawn_blocking` and awaits async ones on the runtime.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let fn_name = func.sig.ident.clone();

    // Parse the optional attribute. Three accepted shapes (see doc comment):
    // empty, a bare string literal (method name), or a comma list of
    // `key = "value"` pairs (`program` / `name`).
    let mut program = String::new();
    let mut name = fn_name.to_string();
    if !attr.is_empty() {
        if let Ok(lit) = syn::parse::<LitStr>(attr.clone()) {
            name = lit.value();
        } else {
            let parser = Punctuated::<MetaNameValue, Token![,]>::parse_terminated;
            let args = match parser.parse(attr) {
                Ok(a) => a,
                Err(e) => return e.to_compile_error().into(),
            };
            for nv in args {
                let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                let val = match &nv.value {
                    Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => s.value(),
                    other => {
                        return syn::Error::new_spanned(other, "handler arg value must be a string literal")
                            .to_compile_error()
                            .into();
                    }
                };
                match key.as_str() {
                    "program" => program = val,
                    "name" => name = val,
                    _ => {
                        return syn::Error::new_spanned(
                            &nv.path,
                            "unknown handler arg (expected `program` or `name`)",
                        )
                        .to_compile_error()
                        .into();
                    }
                }
            }
        }
    }

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

    // An `async fn` registers as `Kind::Async` — a thunk returning a boxed,
    // ctx-borrowing, `Send` future the host awaits on the runtime. A plain `fn`
    // registers as `Kind::Sync` and runs on `spawn_blocking`.
    let kind = if func.sig.asyncness.is_some() {
        quote! {
            ::arbor_rpc::Kind::Async(
                |__ctx: &(dyn ::core::any::Any + 'static), __params: ::serde_json::Value|
                    -> ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<
                        Output = ::core::result::Result<::serde_json::Value, ::std::string::String>
                    > + ::core::marker::Send + '_>>
                {
                    // Downcast BEFORE the async block: the `&dyn Any` is not `Send`
                    // (`dyn Any` isn't `Sync`), so it must not be held across the
                    // `.await`. Only the typed `&Ctx` (Send when `Ctx: Sync`) and the
                    // owned params cross into the future.
                    let __ctx_typed = __ctx.downcast_ref::<#ctx_ty>();
                    ::std::boxed::Box::pin(async move {
                        let #ctx_pat: &#ctx_ty = __ctx_typed
                            .ok_or_else(|| ::std::string::String::from("rpc: wrong backend context type"))?;
                        #(#decode_stmts)*
                        let __out = #fn_name(#ctx_pat, #(#call_args),*)
                            .await
                            .map_err(|__e| ::std::string::ToString::to_string(&__e))?;
                        ::serde_json::to_value(__out)
                            .map_err(|__e| ::std::string::ToString::to_string(&__e))
                    })
                }
            )
        }
    } else {
        quote! {
            ::arbor_rpc::Kind::Sync(
                |__ctx: &(dyn ::core::any::Any + 'static), __params: ::serde_json::Value|
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
                }
            )
        }
    };

    quote! {
        #func

        ::arbor_rpc::inventory::submit! {
            ::arbor_rpc::Entry {
                program: #program,
                name: #name,
                kind: #kind,
            }
        }
    }
    .into()
}
