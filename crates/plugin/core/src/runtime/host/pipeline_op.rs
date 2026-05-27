//! Pipeline-op invocation (`arbor.pipeline.register_op` from Lua).
//!
//! Used by the pipeline orchestrator (`crate::pipeline::run_lua_op`) as an
//! alternative to `StepKind::Shell` — a Lua function plays the role of a
//! shell command and returns a normalised `PipelineOpResult`.

use mlua::LuaSerdeExt;

use super::PluginHost;

/// Result shape returned by a Lua pipeline op handler, normalised so the
/// orchestrator treats it uniformly with shell runs.
///
/// The Lua handler may return any of:
///   · `return true`                        → exit_code=0
///   · `return false`                       → exit_code=1
///   · `return 42`                          → exit_code=42
///   · `return { exit_code=0, stdout="…" }` → full structured form
///   · `return nil` (or no return)          → exit_code=0
/// Anything else is coerced to exit_code=0 and stringified into stdout.
#[derive(Debug, Clone, Default)]
pub struct PipelineOpResult {
    pub exit_code: i32,
    pub stdout:    String,
    pub stderr:    String,
}

impl PluginHost {
    pub fn invoke_pipeline_op(
        &self,
        target: &str,
        op:     &str,
        params: &serde_json::Value,
        cwd:    &str,
    ) -> std::result::Result<PipelineOpResult, String> {
        let plugin = match self.plugins.iter().find(|p| p.manifest.name == target) {
            Some(p) => p,
            None => return Err(format!("plugin '{target}' is not loaded")),
        };
        if !plugin.is_enabled() {
            return Err(format!("plugin '{target}' is disabled"));
        }
        let reg: mlua::Table = match plugin.lua.globals().get("__arbor_pipeline_ops__") {
            Ok(t) => t,
            Err(_) => return Err(format!(
                "plugin '{target}' registered no pipeline ops (call arbor.pipeline.register_op)"
            )),
        };
        let handler: mlua::Function = match reg.get::<mlua::Value>(op) {
            Ok(mlua::Value::Function(f)) => f,
            _ => return Err(format!("pipeline op '{target}.{op}' is not registered")),
        };
        // Build (params, ctx) args. ctx carries the step cwd + plugin name so
        // handlers can keep relative paths semantically consistent with shell
        // steps (default cwd is the pipeline's run cwd).
        let params_lua = plugin.lua.to_value(params)
            .map_err(|e| format!("params → lua error: {e}"))?;
        let ctx_tab = plugin.lua.create_table()
            .map_err(|e| format!("ctx table error: {e}"))?;
        let _ = ctx_tab.set("cwd", cwd);
        let _ = ctx_tab.set("plugin", target);
        let ret: mlua::Value = handler.call((params_lua, ctx_tab))
            .map_err(|e| format!("handler raised: {e}"))?;
        Ok(coerce_pipeline_op_result(ret))
    }

    /// Return "<plugin>.<op>" for every pipeline op registered by any plugin.
    /// Used by `arbor.pipeline.list_ops()` for debugging.
    pub fn list_all_pipeline_ops(&self) -> Vec<String> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            if !plugin.is_enabled() { continue; }
            let reg: mlua::Result<mlua::Table> = plugin.lua.globals().get("__arbor_pipeline_ops__");
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

/// Accept a permissive set of return shapes from a Lua pipeline op handler:
///   nil / no return       → { exit=0 }
///   boolean               → true=0, false=1
///   integer               → used as exit code
///   string                → stdout, exit=0
///   table { exit_code?, stdout?, stderr? } → structured
fn coerce_pipeline_op_result(v: mlua::Value) -> PipelineOpResult {
    use mlua::Value;
    match v {
        Value::Nil                  => PipelineOpResult::default(),
        Value::Boolean(b)           => PipelineOpResult { exit_code: if b {0} else {1}, ..Default::default() },
        Value::Integer(i)           => PipelineOpResult { exit_code: i as i32, ..Default::default() },
        Value::Number(n)            => PipelineOpResult { exit_code: n as i32, ..Default::default() },
        Value::String(s)            => PipelineOpResult {
            exit_code: 0,
            stdout:    s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            stderr:    String::new(),
        },
        Value::Table(t) => {
            let exit = t.get::<Option<i32>>("exit_code").ok().flatten().unwrap_or(0);
            let out  = t.get::<Option<String>>("stdout").ok().flatten().unwrap_or_default();
            let err  = t.get::<Option<String>>("stderr").ok().flatten().unwrap_or_default();
            PipelineOpResult { exit_code: exit, stdout: out, stderr: err }
        }
        _ => PipelineOpResult::default(),
    }
}
