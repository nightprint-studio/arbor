//! Shell-side `arbor.ui.*` slice. After PR #4 Step 6 the bulk of `arbor.ui`
//! migrated into `arbor_plugin_core::lua_api::ns::ui`; only `branding`
//! remains here because it needs Tauri internals (window-icon API,
//! `AppState.branding`, theme-overlay rebroadcast).
//!
//! The plugin-core `ns::ui::install` creates and publishes the `arbor.ui`
//! table. This installer runs afterwards (the shell preserves ordering in
//! `shell_installers()`) and attaches the branding functions onto that
//! existing table.

mod branding;

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use arbor_plugin_core::prelude::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let ui: Table = arbor.get("ui").map_err(|e| AppError::Plugin(format!(
        "arbor.ui.branding install: arbor.ui table missing (plugin-core ns::ui \
         must install first): {e}"
    )))?;
    branding::install(ctx, lua, &ui)?;
    Ok(())
}
