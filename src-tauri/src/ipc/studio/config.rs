//! `config` domain — read/write of the repo-root `.ron-studio.toml` that
//! backs the Studio sidebar: excludes, per-file schema bindings, registered
//! external locations, and reference-field overrides.
//!
//! Each handler is the body the matching `#[tauri::command] async fn` ran
//! inside `spawn_blocking`, now a plain sync function self-registered under
//! `program = "studio"`. The per-handler `spawn_blocking` is gone — the generic
//! `rpc` command dispatches every handler inside one central `spawn_blocking`
//! (see `crate::commands::rpc_commands`), so the file IO still runs off the
//! Tauri runtime workers. Behavior (load-or-default, save, errors) is identical.

use crate::error::AppError;
use crate::ipc::studio;
use crate::studio::config::{self as studio_config, StudioConfig};
use crate::AppState;

/// Read the repo-root `.ron-studio.toml`. Returns an empty config when
/// the file is missing — useful for the sidebar to seed its UI state
/// without a separate "exists?" round-trip.
#[studio::handler(program = "studio")]
fn studio_get_config(state: &AppState, tab_id: String) -> Result<StudioConfig, AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    studio_config::load(&repo_path)
}

/// Register an external location for the active project. `path` can
/// be a single file or a folder (validated server-side); `label` is
/// an optional human name used for the synthetic
/// `external/<label>/…` prefix in the sidebar tree and binding
/// globs. When omitted, the basename of `path` is used. Idempotent
/// on `path` — re-adding the same absolute path refreshes the label.
#[studio::handler(program = "studio")]
fn studio_add_external(
    state: &AppState,
    tab_id: String,
    path: String,
    label: Option<String>,
) -> Result<(), AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    studio_config::add_external(&mut cfg, &path, label.as_deref());
    studio_config::save(&repo_path, &cfg)?;
    Ok(())
}

/// Drop an external location by `path`. No-op when the entry isn't
/// there; returns `true` when an entry was actually removed so the
/// frontend can skip the rescan in the unchanged case.
#[studio::handler(program = "studio")]
fn studio_remove_external(
    state: &AppState,
    tab_id: String,
    path: String,
) -> Result<bool, AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    let removed = studio_config::remove_external(&mut cfg, &path);
    if removed {
        studio_config::save(&repo_path, &cfg)?;
    }
    Ok(removed)
}

/// Toggle an exclude entry for a single file (by repo-relative path).
/// Returns the new state — `true` means now excluded.
#[studio::handler(program = "studio")]
fn studio_toggle_exclude(
    state: &AppState,
    tab_id: String,
    relative_path: String,
) -> Result<bool, AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    let now = studio_config::toggle_exclude(&mut cfg, &relative_path);
    studio_config::save(&repo_path, &cfg)?;
    Ok(now)
}

/// Bind a `.rs` schema + root type to a single file (a per-file
/// override in `.ron-studio.toml`). The next scan / next time the file
/// is opened in RON Studio, this binding takes effect.
#[studio::handler(program = "studio")]
fn studio_bind_schema(
    state: &AppState,
    tab_id: String,
    relative_path: String,
    rs_file: String,
    root_type: String,
    // When `Some`, replaces the entry's stored reference-field patterns;
    // when `None`, the existing list (if any) is preserved so re-binding
    // via the UI doesn't wipe hand-curated patterns.
    reference_fields: Option<Vec<String>>,
) -> Result<(), AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    studio_config::set_binding(&mut cfg, &relative_path, &rs_file, &root_type, reference_fields);
    studio_config::save(&repo_path, &cfg)?;
    Ok(())
}

/// Toggle a single field name in the reference-field patterns of the
/// override matching `relative_path`. Returns the new state.
///
/// Used by the RON Studio tree's "Mark/Unmark as reference field"
/// context-menu — lets the user define cross-ref keys without leaving
/// the tree view. Falls through to creating a per-file override when
/// no binding exists yet so the next save / index refresh picks it up.
#[studio::handler(program = "studio")]
fn studio_toggle_reference_field(
    state: &AppState,
    tab_id: String,
    relative_path: String,
    field: String,
) -> Result<bool, AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    let (now, _scope) = studio_config::toggle_reference_field(&mut cfg, &relative_path, &field);
    studio_config::save(&repo_path, &cfg)?;
    Ok(now)
}

/// Inverse of `studio_bind_schema` — drops the per-file override.
/// Returns `true` when something was removed.
#[studio::handler(program = "studio")]
fn studio_unbind_schema(
    state: &AppState,
    tab_id: String,
    relative_path: String,
) -> Result<bool, AppError> {
    let repo_path = crate::ipc::resolve_tab_path(state, &tab_id)?;
    let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
    let removed = studio_config::clear_binding(&mut cfg, &relative_path);
    if removed {
        studio_config::save(&repo_path, &cfg)?;
    }
    Ok(removed)
}
