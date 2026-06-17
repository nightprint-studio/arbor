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
    /// The backend program this handler belongs to (the router's product
    /// label, e.g. `"corvus"` / `"platform"`). The empty string is the
    /// default/legacy program — handlers annotated with a bare `#[handler]`
    /// (no `program = …`) register under it. Used by [`registry_for`] so a
    /// shell binary that links several backends' handlers can still hand each
    /// program only its own method set.
    pub program: &'static str,
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

/// Collect every `#[handler]`-annotated function into a method → handler map,
/// across **all** programs. A separate `*-be` binary only links its own
/// program's handlers, so this is already its exact method set; build once and
/// cache (the host holds it behind a `OnceLock`).
pub fn registry() -> HashMap<&'static str, CallFn> {
    inventory::iter::<Entry>().map(|e| (e.name, e.call)).collect()
}

/// Like [`registry`] but only the handlers belonging to `program`. The shell
/// binary links several backends' handlers into one inventory while they await
/// their out-of-process split; this lets each program's in-process dispatcher
/// serve only its own methods (so a method never leaks across the program
/// boundary, and same-named methods in different programs don't collide).
pub fn registry_for(program: &str) -> HashMap<&'static str, CallFn> {
    inventory::iter::<Entry>()
        .filter(|e| e.program == program)
        .map(|e| (e.name, e.call))
        .collect()
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

    // Lives in a different program namespace; invisible to `registry_for("")`.
    #[handler(program = "other", name = "other.tick")]
    fn tick(_ctx: &Ctx) -> Result<i64, String> {
        Ok(7)
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

    #[test]
    fn registry_for_partitions_by_program() {
        // The default (empty) program holds the bare-`#[handler]` entries…
        let default = registry_for("");
        assert!(default.contains_key("test.add"));
        assert!(default.contains_key("ping"));
        assert!(!default.contains_key("other.tick"));

        // …and a named program holds only its own.
        let other = registry_for("other");
        assert!(other.contains_key("other.tick"));
        assert!(!other.contains_key("test.add"));
        assert!(!other.contains_key("ping"));

        // The unfiltered registry still sees everything.
        let all = registry();
        assert!(all.contains_key("test.add"));
        assert!(all.contains_key("other.tick"));
    }

    #[test]
    fn programmed_handler_runs() {
        let call = registry_for("other")["other.tick"];
        let out = call(&Ctx { base: 0 }, serde_json::Value::Null).unwrap();
        assert_eq!(out, serde_json::json!(7));
    }
}
