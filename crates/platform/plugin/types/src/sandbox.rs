//! `[sandbox]` section of `plugin.toml`: the Lua stdlib allowlist applied
//! when constructing the per-plugin sandboxed Lua environment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sandbox {
    /// Standard library modules/functions made available in the sandbox.
    /// Omitting a module removes it entirely. Granular entries like "os.time"
    /// are supported. Defaults to ["string", "table", "math"] when not set.
    #[serde(default = "default_lua_libs")]
    pub lua_libs: Vec<String>,
}

fn default_lua_libs() -> Vec<String> {
    vec![
        "string".into(), "table".into(), "math".into(),
        "os.time".into(), "os.clock".into(), "os.date".into(), "os.difftime".into(),
    ]
}
