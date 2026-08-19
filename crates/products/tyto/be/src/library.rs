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
    /// `record` (mp4) | `screenshot` (still image) | `frames` (image sequence).
    pub kind: String,
    pub target: String,
    pub duration_ms: Option<u64>,
    pub size_bytes: u64,
    pub created_at: i64,
    /// The file, or the `.frames` directory for a sequence.
    pub path: String,
    /// Thumbnail to show in the list. Only a frame sequence has one — a video is
    /// its own poster frame and a screenshot is its own thumbnail.
    pub poster: Option<String>,
}

/// A frame sequence, resolved for playback.
///
/// Every frame's absolute path travels in `frames`, deliberately: the alternative is
/// the player re-deriving `frame_%06d.<ext>` from a directory and an extension, which
/// puts the on-disk naming convention in two languages at once. One of them would
/// eventually drift.
#[derive(Serialize)]
pub struct FrameSequence {
    /// The sequence directory.
    pub dir: String,
    pub width: u32,
    pub height: u32,
    /// Sampling ceiling the recording ran at (the real rate is data, not this).
    pub sample_fps: u32,
    /// Total length in ms — how long the last frame is held.
    pub duration_ms: u64,
    pub target: String,
    pub size_bytes: u64,
    /// Absolute path of every frame, in playback order.
    pub frames: Vec<String>,
    /// Presentation time of each frame, ms from the start (`times[0] == 0`).
    pub times: Vec<u32>,
}

/// List every capture in the output dir (newest first).
#[arbor_rpc::handler]
fn list_captures(_state: &TytoState) -> Result<Vec<Capture>, String> {
    Ok(capture::library::scan(&capture::output_dir()))
}

/// Read a frame sequence for playback: its geometry, its per-frame presentation
/// times and the absolute path of every frame, in order.
#[arbor_rpc::handler]
fn read_frame_sequence(_state: &TytoState, id: String) -> Result<FrameSequence, String> {
    let dir = capture::library::resolve_sequence(&capture::output_dir(), &id)?;
    let m = capture::frames::read_manifest(&dir)?;
    let frames = (0..m.frame_count)
        .map(|i| capture::frames::frame_path(&dir, i, &m.format).to_string_lossy().to_string())
        .collect();
    Ok(FrameSequence {
        dir: dir.to_string_lossy().to_string(),
        width: m.width,
        height: m.height,
        sample_fps: m.sample_fps,
        duration_ms: m.duration_ms,
        target: m.target,
        size_bytes: m.size_bytes,
        frames,
        times: m.times,
    })
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
