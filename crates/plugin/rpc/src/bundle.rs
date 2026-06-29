//! The [`PluginRpc`] bundle — the whole Plugin-Manager RPC surface as one
//! [`RpcBundle`]. A product `*-be` adds it to its dispatch builder with a single
//! `.add(PluginRpc)`, monomorphised for its own [`PluginRpcContext`] adapter:
//!
//! ```ignore
//! let (sync, _async) = arbor_rpc::Builder::<MyRpcCtx>::new()
//!     .add(arbor_plugin_rpc::PluginRpc)
//!     .into_maps();
//! ```
//!
//! Every entry is a non-capturing closure: it downcasts the type-erased context
//! to `&C` ([`cast`]), decodes its JSON args by name, calls the matching generic
//! handler, and re-serialises the result. Because the closures capture nothing
//! (only the type `C`), they coerce to the plain fn-pointers the registry uses —
//! no `inventory`, no per-handler glue in the product.

use std::any::Any;

use arbor_rpc::{decode_field, HandlerEntry, RpcBundle};
use serde::Serialize;
use serde_json::Value;

use crate::context::PluginRpcContext;
use crate::{dispatch, introspect, lifecycle, reload, scheduler};

/// Recover the concrete context from the type-erased reference the dispatcher
/// passes, with the same wrong-type error string the `#[handler]` macro emits.
fn cast<C: PluginRpcContext>(any: &dyn Any) -> Result<&C, String> {
    any.downcast_ref::<C>()
        .ok_or_else(|| "rpc: wrong backend context type".to_string())
}

/// Serialise a handler result to JSON, mapping the serde error to its string.
fn jv<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// The Plugin-Manager RPC surface (read + mutate + dispatch) as a single bundle.
pub struct PluginRpc;

impl<C: PluginRpcContext> RpcBundle<C> for PluginRpc {
    fn handlers(&self) -> Vec<HandlerEntry> {
        vec![
            // ── Introspection (read / reflection) ──────────────────────────
            HandlerEntry::sync("list_plugin_info", |any, _p| {
                jv(introspect::list_plugin_info(cast::<C>(any)?)?)
            }),
            HandlerEntry::sync("plugin_enable_preview", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::plugin_enable_preview(c, decode_field(&p, "name")?)?)
            }),
            HandlerEntry::sync("plugin_disable_preview", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::plugin_disable_preview(c, decode_field(&p, "name")?)?)
            }),
            HandlerEntry::sync("plugin_dependents", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::plugin_dependents(c, decode_field(&p, "name")?)?)
            }),
            HandlerEntry::sync("plugin_dep_graph", |any, _p| {
                jv(introspect::plugin_dep_graph(cast::<C>(any)?)?)
            }),
            HandlerEntry::sync("list_plugin_contributions", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::list_plugin_contributions(c, decode_field(&p, "point")?)?)
            }),
            HandlerEntry::sync("list_contribution_points", |any, _p| {
                jv(introspect::list_contribution_points(cast::<C>(any)?)?)
            }),
            HandlerEntry::sync("list_containers", |any, _p| {
                jv(introspect::list_containers(cast::<C>(any)?)?)
            }),
            HandlerEntry::sync("get_container", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::get_container(c, decode_field(&p, "key")?)?)
            }),
            HandlerEntry::sync("plugin_settings_get", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::plugin_settings_get(c, decode_field(&p, "name")?)?)
            }),
            HandlerEntry::sync("plugin_settings_set_all", |any, p| {
                let c = cast::<C>(any)?;
                jv(introspect::plugin_settings_set_all(
                    c,
                    decode_field(&p, "name")?,
                    decode_field(&p, "values")?,
                )?)
            }),
            // ── Enable / disable ───────────────────────────────────────────
            HandlerEntry::sync("enable_plugin", |any, p| {
                let c = cast::<C>(any)?;
                jv(lifecycle::enable_plugin(c, decode_field(&p, "name")?)?)
            }),
            HandlerEntry::sync("disable_plugin", |any, p| {
                let c = cast::<C>(any)?;
                jv(lifecycle::disable_plugin(c, decode_field(&p, "name")?)?)
            }),
            // ── Reload / master toggle ─────────────────────────────────────
            HandlerEntry::sync("reload_plugins", |any, _p| {
                jv(reload::reload_plugins(cast::<C>(any)?)?)
            }),
            HandlerEntry::sync("set_plugins_enabled", |any, p| {
                let c = cast::<C>(any)?;
                jv(reload::set_plugins_enabled(c, decode_field(&p, "enabled")?)?)
            }),
            // ── Per-plugin scheduler ───────────────────────────────────────
            HandlerEntry::sync("start_plugin_scheduler", |any, p| {
                let c = cast::<C>(any)?;
                jv(scheduler::start_plugin_scheduler(
                    c,
                    decode_field(&p, "name")?,
                    decode_field(&p, "action")?,
                )?)
            }),
            HandlerEntry::sync("stop_plugin_scheduler", |any, p| {
                let c = cast::<C>(any)?;
                jv(scheduler::stop_plugin_scheduler(
                    c,
                    decode_field(&p, "name")?,
                    decode_field(&p, "action")?,
                )?)
            }),
            // ── Runtime dispatch ───────────────────────────────────────────
            HandlerEntry::sync("exec_hook", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::exec_hook(
                    c,
                    decode_field(&p, "hook")?,
                    decode_field(&p, "context_json")?,
                )?)
            }),
            HandlerEntry::sync("fire_plugin_action", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::fire_plugin_action(
                    c,
                    decode_field(&p, "plugin_name")?,
                    decode_field(&p, "action")?,
                    decode_field(&p, "context_json")?,
                )?)
            }),
            HandlerEntry::sync("fire_command", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::fire_command(
                    c,
                    decode_field(&p, "caller_plugin")?,
                    decode_field(&p, "id")?,
                    decode_field(&p, "args")?,
                    decode_field(&p, "context_json")?,
                )?)
            }),
            HandlerEntry::sync("set_active_tab", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::set_active_tab(c, decode_field(&p, "tab_id")?)?)
            }),
            // ── Reverse closure-id dispatch (Model-D event push) ───────────
            HandlerEntry::sync("invoke_plugin_callback", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::invoke_plugin_callback(
                    c,
                    decode_field(&p, "plugin_name")?,
                    decode_field(&p, "callback_id")?,
                    decode_field(&p, "context_json")?,
                )?)
            }),
            HandlerEntry::sync("remove_plugin_callback", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::remove_plugin_callback(
                    c,
                    decode_field(&p, "plugin_name")?,
                    decode_field(&p, "callback_id")?,
                )?)
            }),
            HandlerEntry::sync("invoke_pipeline_op", |any, p| {
                let c = cast::<C>(any)?;
                jv(dispatch::invoke_pipeline_op(
                    c,
                    decode_field(&p, "plugin_name")?,
                    decode_field(&p, "op")?,
                    decode_field(&p, "params_json")?,
                    decode_field(&p, "cwd")?,
                )?)
            }),
        ]
    }
}
