//! `brp` (Bevy Remote Protocol) domain — handlers routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. Behavior
//! (lock held, error, result) is byte-identical — only the call path changed.
//!
//! The pure BRP logic already lives in the Tauri-free [`corvus_brp`] crate
//! (`BrpRegistry`, `BrpStatus`, `BrpSession`); these handlers only hold the
//! `AppState` mutex and delegate, so there is **no crate extraction to do** —
//! the only domain-unique file is this handler module.
//!
//! Only the two **synchronous, `AppError`-returning** commands move here:
//! `brp_disconnect` and `brp_status`. `brp_connect` and `brp_call` stay as
//! inline Tauri commands because they are `async` (the generic dispatch path is
//! synchronous) and return the structured [`BrpCallError`] envelope rather than
//! `AppError` (the registry flattens the error to a plain wire string, which
//! would drop the `kind`/`code`/`data` fields the frontend relies on). Moving
//! them would change observable behavior, so they are intentionally left out.

use corvus_brp::prelude::{BrpRegistry, BrpStatus};

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// Lock the BRP registry on `AppState`, mapping a poisoned mutex to the same
/// `AppError` the inline command produced. Reproduces the old `lock_brp` helper
/// (which took a `State<'_, AppState>`) against the broker's `&AppState` ctx.
fn lock_brp(state: &AppState) -> Result<std::sync::MutexGuard<'_, BrpRegistry>, AppError> {
    state.brp.lock().map_err(|e| {
        tracing::error!("brp mutex poisoned: {e}");
        AppError::MutexPoisoned("brp".into())
    })
}

#[corvus::handler]
fn brp_disconnect(state: &AppState) -> Result<BrpStatus, AppError> {
    let mut reg = lock_brp(state)?;
    reg.clear();
    Ok(BrpStatus::from_session(None))
}

#[corvus::handler]
fn brp_status(state: &AppState) -> Result<BrpStatus, AppError> {
    let reg = lock_brp(state)?;
    Ok(BrpStatus::from_session(reg.session()))
}
