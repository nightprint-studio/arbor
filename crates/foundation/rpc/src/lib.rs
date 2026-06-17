//! `arbor-rpc` — the Model-D backend dispatch core.
//!
//! A product backend exposes its commands as plain functions annotated with
//! [`handler`]:
//!
//! ```ignore
//! #[arbor_rpc::handler("stash.save")]
//! fn stash_save(state: &AppState, tab_id: String, message: Option<String>) -> Result<StashEntry, AppError> { … }
//! ```
//!
//! The macro reads the signature, generates the JSON-argument decode + result
//! serialization, and registers the handler via `inventory` — so [`registry`]
//! returns every annotated handler with **no central list and no per-command
//! `match`**. The first parameter is the backend context (a shared reference);
//! the dispatcher passes it type-erased as `&dyn Any` and the generated thunk
//! downcasts it back. Errors cross the seam as their `Display` string (the wire
//! string the FE matches on).
//!
//! This crate is **product-agnostic** — `corvus-be`, `merula-be`, `sitta-be`
//! all share it. The host wraps [`registry`] in its own cache + transport.
//!
//! ## Public API: use the [`prelude`]

// So the `#[handler]`-generated `::arbor_rpc::…` paths resolve inside this
// crate too (e.g. the tests below), not just in downstream consumers.
extern crate self as arbor_rpc;

pub mod prelude;

use std::any::Any;
use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde_json::Value;

// Re-exports the generated code references as `::arbor_rpc::…`, so a consumer
// only ever depends on `arbor-rpc` (the macro + inventory ride along, like
// `serde` re-exporting `serde_derive`).
pub use arbor_rpc_macros::handler;
pub use inventory;

/// A type-erased handler: `(backend context, JSON params) -> JSON result`.
/// The success path is a JSON value; the error is the handler's `Display`
/// string, carried verbatim to the caller.
pub type CallFn = fn(&(dyn Any + 'static), Value) -> Result<Value, String>;

/// One registered handler. Emitted by [`handler`] via `inventory::submit!`.
pub struct Entry {
    pub name: &'static str,
    pub call: CallFn,
}

inventory::collect!(Entry);

/// Decode one named argument out of the JSON param object. A missing key is
/// treated as `null` (so `Option<_>` args default to `None`); a type mismatch
/// is a clear per-argument error string.
pub fn decode_field<T: DeserializeOwned>(params: &Value, key: &str) -> Result<T, String> {
    let v = params.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| format!("arg '{key}': {e}"))
}

/// Collect every `#[handler]`-annotated function into a method → handler map.
/// Build once and cache (the host holds it behind a `OnceLock`).
pub fn registry() -> HashMap<&'static str, CallFn> {
    inventory::iter::<Entry>().map(|e| (e.name, e.call)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ctx {
        base: i64,
    }

    #[handler("test.add")]
    fn add(ctx: &Ctx, lhs: i64, rhs: Option<i64>) -> Result<i64, String> {
        Ok(ctx.base + lhs + rhs.unwrap_or(0))
    }

    // No explicit name → registers under the function's own name ("ping").
    #[handler]
    fn ping(_ctx: &Ctx) -> Result<&'static str, String> {
        Ok("pong")
    }

    #[test]
    fn handler_registers_and_decodes_args() {
        let reg = registry();
        let call = reg.get("test.add").expect("handler registered");
        let ctx = Ctx { base: 100 };
        // `rhs` omitted → Option defaults to None.
        let out = call(&ctx, serde_json::json!({ "lhs": 5 })).unwrap();
        assert_eq!(out, serde_json::json!(105));
        let out = call(&ctx, serde_json::json!({ "lhs": 5, "rhs": 20 })).unwrap();
        assert_eq!(out, serde_json::json!(125));
    }

    #[test]
    fn handler_name_defaults_to_fn_name() {
        let reg = registry();
        let call = reg.get("ping").expect("registered under its fn name");
        let out = call(&Ctx { base: 0 }, serde_json::Value::Null).unwrap();
        assert_eq!(out, serde_json::json!("pong"));
    }

    #[test]
    fn wrong_context_type_is_an_error() {
        let reg = registry();
        let call = reg.get("test.add").unwrap();
        let err = call(&"not a ctx", serde_json::json!({ "lhs": 1 })).unwrap_err();
        assert!(err.contains("context"));
    }

    #[test]
    fn bad_arg_type_is_a_clear_error() {
        let reg = registry();
        let call = reg.get("test.add").unwrap();
        let ctx = Ctx { base: 0 };
        let err = call(&ctx, serde_json::json!({ "lhs": "nope" })).unwrap_err();
        assert!(err.contains("lhs"));
    }
}
