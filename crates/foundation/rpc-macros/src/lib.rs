//! `#[handler("method.name")]` — the proc-macro behind `arbor-rpc`.
//!
//! Annotating a backend handler turns it into a self-registering RPC entry: the
//! macro reads the function signature and generates the JSON-argument decode,
//! the result serialization, and an `inventory::submit!` so the handler shows up
//! in `arbor_rpc::registry()` with no central list. The same trick
//! `#[tauri::command]` uses, retargeted at the Model-D generic dispatch.
//!
//! The optional `mcp(...)` group additionally publishes the handler as a **tool** an
//! AI client can discover and call — with a description, a JSON Schema and a safety
//! class. See [`handler`].
//!
//! Reach it through `arbor_rpc::handler` (this crate is re-exported there).

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Attribute, Expr, ExprLit, FnArg, GenericArgument, ItemFn, Lit, LitStr, Meta,
    Pat, PathArguments, Token, Type,
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
///
/// # Exposing a handler as an AI tool — `mcp(...)`
///
/// ```ignore
/// /// Read a file's text, decoded with the project's resolved encoding.
/// /// Returns the text plus the encoding that applied.
/// #[arbor_rpc::handler(mcp(title = "Read a project file", safety = read))]
/// fn bennu_read_file(ctx: &BennuState, args: ReadFileArgs) -> Result<FileContents, String> { … }
/// ```
///
/// Keys inside `mcp(...)`, all optional except `safety`:
///
/// | Key | Meaning |
/// |---|---|
/// | `name = "…"` | The tool's name, when the method's isn't unique across products or legible to a model. Defaults to the method name. |
/// | `title = "…"` | Human label. Defaults to the tool name. |
/// | `description = "…"` | Overrides the `///` doc comment, which is the default source. |
/// | `safety = read \| write \| destructive` | **Required.** Drives the host's allow/ask/deny policy. |
/// | `idempotent` / `idempotent = false` | Defaults to `true` for `read`, `false` otherwise. |
/// | `open_world` | The tool reaches outside this machine. Defaults to `false`. |
/// | `output = json \| text \| image` | How the host renders the result. Defaults to `json`. |
/// | `schema = "{…}"` | Literal JSON Schema, for the rare shape neither path below covers. |
///
/// **Schema generation.** A handler taking a single non-scalar argument (the
/// `fn(ctx, args: FooArgs)` convention) publishes `FooArgs`'s own schema *flattened*, and
/// the host re-wraps the arguments under that parameter name before dispatch — a model
/// should never see a wrapper that exists for the seam's benefit. Any other signature
/// composes an object schema from the individual arguments. Either way the types must
/// implement `schemars::JsonSchema`: derive it on the args struct.
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let fn_name = func.sig.ident.clone();

    // Parse the optional attribute. Accepted shapes (see doc comment): empty, a bare
    // string literal (method name), or a comma list mixing `key = "value"` pairs
    // (`program` / `name`) with the `mcp(...)` group.
    let mut program = String::new();
    let mut name = fn_name.to_string();
    let mut mcp: Option<McpArgs> = None;

    if !attr.is_empty() {
        if let Ok(lit) = syn::parse::<LitStr>(attr.clone()) {
            name = lit.value();
        } else {
            let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
            let args = match parser.parse(attr) {
                Ok(a) => a,
                Err(e) => return e.to_compile_error().into(),
            };
            for meta in args {
                match &meta {
                    Meta::NameValue(nv) => {
                        let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                        let val = match str_of(&nv.value) {
                            Some(v) => v,
                            None => return err(&nv.value, "handler arg value must be a string literal"),
                        };
                        match key.as_str() {
                            "program" => program = val,
                            "name" => name = val,
                            _ => return err(&nv.path, "unknown handler arg (expected `program`, `name` or `mcp(...)`)"),
                        }
                    }
                    Meta::List(list) if list.path.is_ident("mcp") => {
                        match parse_mcp(list) {
                            Ok(parsed) => mcp = Some(parsed),
                            Err(e) => return e.to_compile_error().into(),
                        }
                    }
                    other => return err(other, "unknown handler arg (expected `program`, `name` or `mcp(...)`)"),
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
                return err(other, "first handler arg must be a shared reference context, e.g. `&AppState`")
            }
        },
        _ => return err(&func.sig, "handler needs a context first arg (e.g. `state: &AppState`)"),
    };

    // Remaining params = JSON-decodable arguments.
    let mut decode_stmts = Vec::new();
    let mut call_args = Vec::new();
    let mut arg_types: Vec<(String, Type)> = Vec::new();
    for arg in inputs {
        let FnArg::Typed(pt) = arg else { continue };
        let ident = match &*pt.pat {
            Pat::Ident(pi) => pi.ident.clone(),
            other => return err(other, "handler args must be plain identifiers"),
        };
        let ty = pt.ty.clone();
        let key = ident.to_string();
        decode_stmts.push(quote! {
            let #ident: #ty = ::arbor_rpc::decode_field(&__params, #key)?;
        });
        call_args.push(ident);
        arg_types.push((key, (*ty).clone()));
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

    // ── The optional tool half ──────────────────────────────────────────────
    let (schema_fn, mcp_expr) = match &mcp {
        None => (quote! {}, quote! { ::core::option::Option::None }),
        Some(m) => {
            let schema_ident = format_ident!("__arbor_tool_schema_{}", fn_name);
            let (schema_body, wrap_in) = match schema_source(m, &arg_types) {
                Ok(pair) => pair,
                Err(ts) => return ts,
            };
            let description = match &m.description {
                Some(d) => d.clone(),
                None => doc_of(&func.attrs),
            };
            if description.trim().is_empty() {
                return err(
                    &func.sig,
                    "an mcp(...) handler needs a description: write a /// doc comment on it, \
                     or pass description = \"…\". It is the only thing a model reads before \
                     deciding to call this tool.",
                );
            }
            // The tool name defaults to the method name; `mcp(name = …)` separates them
            // when the method name isn't unique or legible across products.
            let tool_name = m.name.clone().unwrap_or_else(|| name.clone());
            let name_expr = match &m.name {
                Some(n) => quote! { ::core::option::Option::Some(#n) },
                None => quote! { ::core::option::Option::None },
            };
            let title = m.title.clone().unwrap_or(tool_name);
            let safety = match m.safety.as_deref() {
                Some("read") => quote! { ::arbor_rpc::Safety::Read },
                Some("write") => quote! { ::arbor_rpc::Safety::Write },
                Some("destructive") => quote! { ::arbor_rpc::Safety::Destructive },
                Some(_) => return err(&func.sig, "mcp(safety = …) must be `read`, `write` or `destructive`"),
                None => {
                    return err(
                        &func.sig,
                        "mcp(...) requires `safety = read | write | destructive` — the host \
                         cannot decide whether to ask the user without it",
                    )
                }
            };
            // Reads are idempotent unless said otherwise; anything that mutates is not.
            let idempotent = m
                .idempotent
                .unwrap_or(matches!(m.safety.as_deref(), Some("read")));
            let open_world = m.open_world.unwrap_or(false);
            let output = match m.output.as_deref() {
                None | Some("json") => quote! { ::arbor_rpc::ToolOutput::Json },
                Some("text") => quote! { ::arbor_rpc::ToolOutput::Text },
                Some("image") => quote! { ::arbor_rpc::ToolOutput::Image },
                Some(_) => return err(&func.sig, "mcp(output = …) must be `json`, `text` or `image`"),
            };
            let wrap_expr = match wrap_in {
                Some(key) => quote! { ::core::option::Option::Some(#key) },
                None => quote! { ::core::option::Option::None },
            };
            (
                quote! {
                    #[doc(hidden)]
                    #[allow(non_snake_case)]
                    fn #schema_ident() -> ::serde_json::Value { #schema_body }
                },
                quote! {
                    ::core::option::Option::Some(::arbor_rpc::ToolMeta {
                        name: #name_expr,
                        title: #title,
                        description: #description,
                        safety: #safety,
                        idempotent: #idempotent,
                        open_world: #open_world,
                        wrap_in: #wrap_expr,
                        output: #output,
                        schema: #schema_ident,
                    })
                },
            )
        }
    };

    quote! {
        #func

        #schema_fn

        ::arbor_rpc::inventory::submit! {
            ::arbor_rpc::Entry {
                program: #program,
                name: #name,
                kind: #kind,
                mcp: #mcp_expr,
            }
        }
    }
    .into()
}

/// The parsed `mcp(...)` group.
#[derive(Default)]
struct McpArgs {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    safety: Option<String>,
    idempotent: Option<bool>,
    open_world: Option<bool>,
    output: Option<String>,
    schema: Option<String>,
}

fn parse_mcp(list: &syn::MetaList) -> syn::Result<McpArgs> {
    let mut out = McpArgs::default();
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    for meta in list.parse_args_with(parser)? {
        match &meta {
            // Bare flag: `idempotent`, `open_world`.
            Meta::Path(p) => {
                let key = p.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match key.as_str() {
                    "idempotent" => out.idempotent = Some(true),
                    "open_world" => out.open_world = Some(true),
                    _ => return Err(syn::Error::new_spanned(p, "unknown mcp(...) flag")),
                }
            }
            Meta::NameValue(nv) => {
                let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
                match key.as_str() {
                    "name" => out.name = Some(require_str(&nv.value)?),
                    "title" => out.title = Some(require_str(&nv.value)?),
                    "description" => out.description = Some(require_str(&nv.value)?),
                    "schema" => out.schema = Some(require_str(&nv.value)?),
                    // `safety = read` is a bare word, not a string: it is a closed set,
                    // and a typo should be a compile error rather than a runtime surprise.
                    "safety" => out.safety = Some(require_word(&nv.value)?),
                    "output" => out.output = Some(require_word(&nv.value)?),
                    "idempotent" => out.idempotent = Some(require_bool(&nv.value)?),
                    "open_world" => out.open_world = Some(require_bool(&nv.value)?),
                    _ => return Err(syn::Error::new_spanned(&nv.path, "unknown mcp(...) key")),
                }
            }
            other => return Err(syn::Error::new_spanned(other, "unexpected mcp(...) entry")),
        }
    }
    Ok(out)
}

/// Decide how this handler's input schema is built, and whether the host must re-wrap
/// the model's flat arguments under a parameter name before dispatch.
type SchemaSource = (proc_macro2::TokenStream, Option<String>);

fn schema_source(m: &McpArgs, args: &[(String, Type)]) -> Result<SchemaSource, TokenStream> {
    // Escape hatch: a literal schema wins over everything.
    if let Some(literal) = &m.schema {
        return Ok((
            quote! {
                ::serde_json::from_str(#literal)
                    .expect("handler mcp(schema = ...) is not valid JSON")
            },
            None,
        ));
    }

    // The `fn(ctx, args: FooArgs)` convention: publish FooArgs flattened, re-wrap on
    // the way in. See `ToolMeta::wrap_in` for why the wrapper is hidden.
    if args.len() == 1 && !is_scalarish(&args[0].1) {
        let (key, ty) = &args[0];
        return Ok((quote! { ::arbor_rpc::schema_of::<#ty>() }, Some(key.clone())));
    }

    // Otherwise compose an object out of the individual arguments.
    let fields = args.iter().map(|(key, ty)| {
        let (inner, required) = match option_inner(ty) {
            Some(inner) => (inner.clone(), false),
            None => (ty.clone(), true),
        };
        quote! {
            ::arbor_rpc::ToolField {
                name: #key,
                schema: ::arbor_rpc::schema_of::<#inner>,
                required: #required,
            }
        }
    });
    Ok((quote! { ::arbor_rpc::object_schema(&[ #(#fields),* ]) }, None))
}

/// Whether a type is a leaf as far as schema generation is concerned. A single
/// non-scalar argument is the struct-args convention; a single scalar one is just an
/// argument that happens to be alone.
fn is_scalarish(ty: &Type) -> bool {
    let Type::Path(tp) = ty else { return true };
    let Some(seg) = tp.path.segments.last() else { return true };
    matches!(
        seg.ident.to_string().as_str(),
        "String"
            | "str"
            | "bool"
            | "char"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "Option"
            | "Vec"
            | "HashMap"
            | "BTreeMap"
            | "PathBuf"
            | "Value"
    )
}

/// `Option<T>` → `T`.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(ab) = &seg.arguments else { return None };
    ab.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t),
        _ => None,
    })
}

/// The function's `///` doc comment, joined back into a paragraph. This is the default
/// tool description: keeping the two in one place is the only way they stay in sync.
fn doc_of(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(nv) = &attr.meta {
            if let Some(text) = str_of(&nv.value) {
                lines.push(text.trim().to_string());
            }
        }
    }
    lines.join("\n").trim().to_string()
}

fn str_of(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => Some(s.value()),
        _ => None,
    }
}

fn require_str(expr: &Expr) -> syn::Result<String> {
    str_of(expr).ok_or_else(|| syn::Error::new_spanned(expr, "expected a string literal"))
}

fn require_bool(expr: &Expr) -> syn::Result<bool> {
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Bool(b), .. }) => Ok(b.value()),
        _ => Err(syn::Error::new_spanned(expr, "expected `true` or `false`")),
    }
}

fn require_word(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Path(p) => p
            .path
            .get_ident()
            .map(|i| i.to_string())
            .ok_or_else(|| syn::Error::new_spanned(expr, "expected a bare word")),
        _ => Err(syn::Error::new_spanned(expr, "expected a bare word, e.g. `read`")),
    }
}

fn err<T: quote::ToTokens>(tokens: T, message: &str) -> TokenStream {
    syn::Error::new_spanned(tokens, message).to_compile_error().into()
}
