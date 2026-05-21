//! `arbor.ui` orchestrator. Each sub-module installs one slice of the
//! `arbor.ui.*` surface on the shared `ui` table.

mod activity_bar;
mod autocomplete;
mod branding;
mod confirm;
mod container;
mod contribute;
mod form;
mod icon;
mod misc;
mod operation;
mod pick_file;
mod settings_form;
mod sugar;
mod tree;

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let ui = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    pick_file::install(ctx, lua, &ui)?;
    form::install(ctx, lua, &ui)?;
    confirm::install(ctx, lua, &ui)?;
    operation::install(ctx, lua, &ui)?;

    // Sugar APIs that fan into the contribution registry.
    sugar::install(ctx, lua, &ui)?;

    activity_bar::install(ctx, lua, &ui)?;
    autocomplete::install(ctx, lua, &ui)?;
    branding::install(ctx, lua, &ui)?;
    misc::install(ctx, lua, &ui)?;

    tree::install(ctx, lua, &ui)?;
    contribute::install(ctx, lua, &ui)?;
    settings_form::install(ctx, lua, &ui)?;
    icon::install(ctx, lua, &ui)?;
    container::install(ctx, lua, &ui)?;

    arbor.set("ui", ui).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
