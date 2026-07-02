//! `sources` domain — enumerate capture targets + preview the selected one. Thin
//! wrappers over [`crate::capture`] (windows-capture monitors/windows + cpal mic
//! inputs; scap does the capture side).

use serde::{Deserialize, Serialize};
use tyto_core::prelude::TytoState;

use crate::capture::{self, CaptureTarget, CropRect};
use crate::region::PixelRect;

/// A capturable display.
#[derive(Serialize)]
pub struct MonitorSource {
    pub id: String,
    pub name: String,
    pub resolution: String,
    pub scale: f64,
    pub primary: bool,
}

/// A capturable application window.
#[derive(Serialize)]
pub struct WindowSource {
    pub id: String,
    pub title: String,
    pub app: String,
}

/// An audio input device (microphone).
#[derive(Serialize)]
pub struct AudioInput {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Monitors + windows in one round-trip (both are queried together by the picker).
#[derive(Serialize)]
pub struct CaptureSources {
    pub monitors: Vec<MonitorSource>,
    pub windows: Vec<WindowSource>,
}

/// Enumerate monitors + capturable windows.
#[arbor_rpc::handler]
fn list_capture_sources(_state: &TytoState) -> Result<CaptureSources, String> {
    Ok(crate::capture::source::list_capture_sources())
}

/// Enumerate audio input devices (microphones). System audio is not a device here
/// — it's a separate WASAPI loopback toggle.
#[arbor_rpc::handler]
fn list_audio_inputs(_state: &TytoState) -> Result<Vec<AudioInput>, String> {
    Ok(crate::capture::source::list_audio_inputs())
}

/// Parameters for [`preview_source`] — the same target shape as a capture.
#[derive(Deserialize)]
pub struct PreviewArgs {
    pub target_kind: String,
    pub source_id: Option<String>,
    pub region: Option<PixelRect>,
}

/// Grab a downscaled preview thumbnail of the selected source (monitor / window /
/// region) to a temp PNG; returns its path for the picker to show.
#[arbor_rpc::handler]
fn preview_source(_state: &TytoState, args: PreviewArgs) -> Result<String, String> {
    let region = args.region.map(|r| CropRect { x: r.x, y: r.y, w: r.w, h: r.h });
    let target = CaptureTarget::resolve(&args.target_kind, args.source_id.as_deref(), region)?;
    let path = capture::screenshot::preview(&target)?;
    Ok(path.to_string_lossy().to_string())
}
