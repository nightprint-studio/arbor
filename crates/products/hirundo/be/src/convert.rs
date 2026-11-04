//! Conversions the window asks for rather than re-implements: cURL in and out, and the bulk-edit text.

use hirundo_model::prelude::{kv_to_text, parse_curl, parse_kv_text, to_curl, HttpRequest, KeyValue};

use crate::context::{build, Target};
use hirundo_core::prelude::HirundoState;

/// `curl …` (or Chrome's "Copy as cURL") → a request, not yet saved.
#[arbor_rpc::handler]
fn curl_import(_state: &HirundoState, text: String) -> Result<HttpRequest, String> {
    parse_curl(&text)
}

/// A request as a `curl` command. `resolved` fills the variables in from the environment; off keeps the
/// `{{placeholders}}`, so the command can be shared without the values.
#[arbor_rpc::handler]
fn curl_export(state: &HirundoState, target: Target, resolved: bool) -> Result<String, String> {
    Ok(to_curl(&build(state, &target, resolved)?.prepared))
}

/// Bulk-edit text → rows (`key: value`, `k=v&k2=v2`, a block of headers copied from a browser).
#[arbor_rpc::handler]
fn kv_parse(_state: &HirundoState, text: String) -> Result<Vec<KeyValue>, String> {
    Ok(parse_kv_text(&text))
}

/// Rows → bulk-edit text.
#[arbor_rpc::handler]
fn kv_format(_state: &HirundoState, rows: Vec<KeyValue>) -> Result<String, String> {
    Ok(kv_to_text(&rows))
}
