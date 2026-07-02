//! `library` domain — the saved-captures library, backed by the output dir
//! ([`crate::capture::library`]). reveal/open route through the shell reverse
//! channel (`__open_path`).

use serde::Serialize;
use serde_json::json;
use tyto_core::prelude::TytoState;

use crate::capture;

/// One saved capture on disk.
#[derive(Serialize)]
pub struct Capture {
    pub id: String,
    pub name: String,
    /// `record` | `screenshot`.
    pub kind: String,
    pub target: String,
    pub duration_ms: Option<u64>,
    pub size_bytes: u64,
    pub created_at: i64,
    pub path: String,
}

/// List every capture in the output dir (newest first).
#[arbor_rpc::handler]
fn list_captures(_state: &TytoState) -> Result<Vec<Capture>, String> {
    Ok(capture::library::scan(&capture::output_dir()))
}

/// Rename a capture on disk (extension preserved).
#[arbor_rpc::handler]
fn rename_capture(_state: &TytoState, id: String, name: String) -> Result<(), String> {
    capture::library::rename(&capture::output_dir(), &id, &name)
}

/// Delete a capture.
#[arbor_rpc::handler]
fn remove_capture(_state: &TytoState, id: String) -> Result<(), String> {
    capture::library::remove(&capture::output_dir(), &id)
}

/// Delete every capture in the output dir.
#[arbor_rpc::handler]
fn clear_captures(_state: &TytoState) -> Result<(), String> {
    let dir = capture::output_dir();
    for c in capture::library::scan(&dir) {
        let _ = capture::library::remove(&dir, &c.id);
    }
    Ok(())
}

/// Reveal the output directory (where captures are saved) in the OS file manager,
/// creating it first if it doesn't exist yet.
#[arbor_rpc::handler]
fn reveal_output(state: &TytoState) -> Result<(), String> {
    let dir = capture::output_dir();
    let _ = std::fs::create_dir_all(&dir);
    state
        .host_call("__open_path", json!({ "path": dir.to_string_lossy() }))
        .map(|_| ())
}

/// Reveal a capture in the OS file manager (via the reverse channel).
#[arbor_rpc::handler]
fn reveal_capture(state: &TytoState, id: String) -> Result<(), String> {
    let p = capture::library::resolve_path(&capture::output_dir(), &id)?;
    state
        .host_call("__open_path", json!({ "path": p.to_string_lossy() }))
        .map(|_| ())
}

/// Open a capture. Today this reveals it in the file manager (the shell's
/// `__open_path` arm); a dedicated open-with-default-app arm is a small follow-up.
#[arbor_rpc::handler]
fn open_capture(state: &TytoState, id: String) -> Result<(), String> {
    let p = capture::library::resolve_path(&capture::output_dir(), &id)?;
    state
        .host_call("__open_path", json!({ "path": p.to_string_lossy() }))
        .map(|_| ())
}
