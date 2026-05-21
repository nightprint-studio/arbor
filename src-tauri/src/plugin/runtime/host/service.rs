//! Cross-plugin service calls (`arbor.service.*`): synchronous invocation,
//! deferred response delivery, and discovery.

use mlua::LuaSerdeExt;

use super::PluginHost;

/// Error raised when a cross-plugin service call fails.
#[derive(Debug)]
pub enum ServiceError {
    NotFound(String),
    PluginDisabled(String),
    HandlerError(String),
}

impl ServiceError {
    pub fn kind(&self) -> &'static str {
        match self {
            ServiceError::NotFound(_)       => "not_found",
            ServiceError::PluginDisabled(_) => "plugin_disabled",
            ServiceError::HandlerError(_)   => "handler_error",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            ServiceError::NotFound(m)       => m,
            ServiceError::PluginDisabled(m) => m,
            ServiceError::HandlerError(m)   => m,
        }
    }
}

impl PluginHost {
    /// Look up `target.__arbor_services__[method]` and invoke it with the
    /// given JSON-encoded args. Returns the handler's return value (serialised
    /// back to JSON) or a typed ServiceError.
    pub fn invoke_service(
        &self,
        target: &str,
        method: &str,
        args_json: &serde_json::Value,
    ) -> std::result::Result<serde_json::Value, ServiceError> {
        let plugin = match self.plugins.iter().find(|p| p.manifest.name == target) {
            Some(p) => p,
            None => return Err(ServiceError::NotFound(format!(
                "plugin '{target}' is not loaded"
            ))),
        };
        if !plugin.is_enabled() {
            return Err(ServiceError::PluginDisabled(format!(
                "plugin '{target}' is disabled"
            )));
        }
        let reg: mlua::Table = match plugin.lua.globals().get("__arbor_services__") {
            Ok(t) => t,
            Err(_) => return Err(ServiceError::NotFound(format!(
                "plugin '{target}' has no service registry"
            ))),
        };
        let handler: mlua::Function = match reg.get::<mlua::Value>(method) {
            Ok(mlua::Value::Function(f)) => f,
            _ => return Err(ServiceError::NotFound(format!(
                "service '{target}.{method}' is not registered"
            ))),
        };
        let args_lua = plugin.lua.to_value(args_json)
            .unwrap_or(mlua::Value::Nil);
        let ret: mlua::Value = handler.call(args_lua)
            .map_err(|e| ServiceError::HandlerError(e.to_string()))?;
        let ret_json: serde_json::Value = plugin.lua.from_value(ret)
            .map_err(|e| ServiceError::HandlerError(format!("return serialise error: {e}")))?;
        Ok(ret_json)
    }

    /// Pop a registered callback from `caller.__arbor_service_callbacks__[id]`
    /// and invoke it with `(ok, payload)`. A missing callback is silently
    /// ignored (e.g. the caller didn't pass one, or it was already consumed).
    pub fn deliver_service_response(
        &self,
        caller: &str,
        call_id: &str,
        ok: bool,
        payload: &serde_json::Value,
    ) {
        let plugin = match self.plugins.iter().find(|p| p.manifest.name == caller) {
            Some(p) => p,
            None => return,
        };
        if !plugin.is_enabled() { return; }
        let cbs: mlua::Table = match plugin.lua.globals().get("__arbor_service_callbacks__") {
            Ok(t) => t,
            Err(_) => return,
        };
        let cb: mlua::Function = match cbs.get::<mlua::Value>(call_id) {
            Ok(mlua::Value::Function(f)) => f,
            _ => return,
        };
        let _ = cbs.set(call_id, mlua::Value::Nil);
        let payload_lua = plugin.lua.to_value(payload).unwrap_or(mlua::Value::Nil);
        if let Err(e) = cb.call::<mlua::Value>((ok, payload_lua)) {
            tracing::warn!(target: "plugin", "service callback error in '{caller}': {e}");
        }
    }

    /// Return "<plugin>.<method>" for every service exported by any enabled
    /// plugin. Used by `arbor.service.list()` for debugging / discovery.
    pub fn list_all_services(&self) -> Vec<String> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            if !plugin.is_enabled() { continue; }
            let reg: mlua::Result<mlua::Table> = plugin.lua.globals().get("__arbor_services__");
            if let Ok(reg) = reg {
                for pair in reg.pairs::<String, mlua::Function>() {
                    if let Ok((k, _)) = pair {
                        out.push(format!("{}.{}", plugin.manifest.name, k));
                    }
                }
            }
        }
        out
    }
}
