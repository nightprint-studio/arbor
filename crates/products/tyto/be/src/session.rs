//! `session` domain — the recording session lifecycle, driving the capture engine
//! ([`crate::capture::ENGINE`]). Start/stop/pause a recording, take a screenshot,
//! poll state. Progress telemetry (`tyto://recording-progress`) is emitted by the
//! engine's progress thread through the event sink.
//!
//! Recorder lifecycle hooks (`on_recording_started`, …) are NOT fired yet — they
//! join the shared hook catalog (+ SDK/docs) in a follow-up; nothing invents a
//! hook that isn't in the catalog.

use serde::{Deserialize, Serialize};
use tyto_core::config::load as load_cfg;
use tyto_core::prelude::TytoState;

use crate::capture::{self, target::CropRect, session::{RecordingOutput, StartConfig}};
use crate::region::PixelRect;

/// Parameters for [`start_recording`] and [`take_screenshot`].
#[derive(Deserialize)]
pub struct StartRecordingArgs {
    /// `monitor` | `window` | `region`.
    pub target_kind: String,
    /// Source id for monitor/window (`mon-<id>` / `win-<id>`); ignored for region.
    pub source_id: Option<String>,
    pub fps: Option<u32>,
    pub quality: Option<String>,
    pub system_audio: Option<bool>,
    pub mic_id: Option<String>,
    /// Physical-pixel rectangle (monitor-local) when `target_kind == "region"`.
    pub region: Option<PixelRect>,
    /// Freehand mask polygon, **physical & region-local** pixels (0-based within the
    /// crop). Only honoured by [`take_screenshot`] when `target_kind == "region"`: the
    /// cropped image gets alpha=0 outside the polygon and is forced to PNG. Recordings
    /// ignore it (freehand records the bounding box). `None`/empty = no mask.
    /// `#[serde(default)]` so `start_recording` (which never sends this key) deserializes.
    #[serde(default)]
    pub mask_points: Option<Vec<[i32; 2]>>,
    /// What the recording should produce: `video` (H.264 mp4) or `frames` (a
    /// deduplicated, timestamped image sequence). Omitted = the persisted default.
    ///
    /// Only the CHOICE travels on the wire. How a sequence is written — frame format,
    /// sampling ceiling, downscale — comes from the persisted config, exactly like the
    /// filename template and the screenshot format do: those are settings the user set
    /// once, not per-capture arguments.
    #[serde(default)]
    pub output: Option<String>,
}

/// The current session snapshot the frontend polls.
#[derive(Serialize)]
pub struct SessionState {
    pub session_id: Option<String>,
    pub recording: bool,
    pub paused: bool,
    pub elapsed_ms: u64,
    /// What the running session produces: `video` | `frames`.
    pub output: String,
}

/// Resolve the sink from the requested output (or the persisted default) plus the
/// persisted frame-sequence settings. Anything unrecognised falls back to video —
/// a typo must not silently change what a capture produces.
fn output_for(requested: Option<&str>, cfg: &tyto_core::prelude::TytoConfig) -> RecordingOutput {
    let want = requested.unwrap_or(cfg.record_output.as_str());
    if want.eq_ignore_ascii_case("frames") {
        RecordingOutput::Frames {
            format: cfg.frames.format.clone(),
            sample_fps: cfg.frames.sample_fps.clamp(1, 60),
            max_width: cfg.frames.max_width,
        }
    } else {
        // Already derived from the preset by `TytoConfig::normalize` — the table
        // lives in `tyto-core`, and this reads what it produced.
        RecordingOutput::Video { bitrate_kbps: cfg.encoding.bitrate_kbps }
    }
}

fn crop_from(region: Option<PixelRect>) -> Option<CropRect> {
    region.map(|r| CropRect { x: r.x, y: r.y, w: r.w, h: r.h })
}

fn build_start_config(args: &StartRecordingArgs) -> StartConfig {
    let mut cfg = load_cfg();
    let fps = args.fps.unwrap_or(cfg.capture.fps).clamp(1, 240);
    // A per-call quality overrides the persisted one; re-normalizing makes the
    // bitrate follow it, through the same path a saved config takes.
    if let Some(q) = args.quality.clone() {
        cfg.encoding.quality = q;
        cfg.normalize();
    }
    let output = output_for(args.output.as_deref(), &cfg);
    let mic_id = args.mic_id.clone().filter(|s| !s.trim().is_empty());
    let system_audio = args.system_audio.unwrap_or(cfg.capture.system_audio);
    StartConfig {
        target_kind: args.target_kind.clone(),
        source_id: args.source_id.clone(),
        region: crop_from(args.region),
        fps,
        mic_id,
        system_audio,
        out_dir: capture::output_dir(),
        filename_template: cfg.output.filename_template.clone(),
        target_label: args.source_id.clone().unwrap_or_else(|| args.target_kind.clone()),
        output,
    }
}

/// Begin a recording session; returns the session id. The engine starts emitting
/// `tyto://recording-progress`.
#[arbor_rpc::handler]
fn start_recording(state: &TytoState, args: StartRecordingArgs) -> Result<String, String> {
    let cfg = build_start_config(&args);
    capture::ENGINE.start(cfg, state.event_sink())
}

/// Stop the active session; joins the encoder + mic and muxes the final file.
#[arbor_rpc::handler]
fn stop_recording(_state: &TytoState) -> Result<(), String> {
    capture::ENGINE.stop().map(|_| ())
}

/// Pause / resume the active session.
#[arbor_rpc::handler]
fn pause_recording(_state: &TytoState, paused: bool) -> Result<(), String> {
    capture::ENGINE.pause(paused)
}

/// Capture a single screenshot of the given target; returns the saved file path.
///
/// A freehand region carries `mask_points` (physical, region-local): the cropped
/// image is punched to transparent outside that polygon and forced to PNG (alpha).
#[arbor_rpc::handler]
fn take_screenshot(_state: &TytoState, args: StartRecordingArgs) -> Result<String, String> {
    let target = capture::CaptureTarget::resolve(&args.target_kind, args.source_id.as_deref(), crop_from(args.region))?;
    let cfg = load_cfg();
    // Only a region grab honours the freehand mask (it needs the crop to be region-local).
    let mask = if args.target_kind == "region" {
        args.mask_points.filter(|p| p.len() >= 3)
    } else {
        None
    };
    let path = capture::screenshot::take(
        &target,
        &capture::output_dir(),
        &cfg.output.filename_template,
        mask.as_deref(),
    )?;
    Ok(path.to_string_lossy().to_string())
}

/// Report whether a screen recording is currently running, and for how long.
///
/// Check this before assuming the screen is idle: a recording started by the user is
/// invisible to anything else, and interfering with one is not recoverable.
#[arbor_rpc::handler(mcp(
    name = "tyto_recording_state",
    title = "Check the recording state",
    safety = read,
))]
fn session_state(_state: &TytoState) -> Result<SessionState, String> {
    let s = capture::ENGINE.snapshot();
    Ok(SessionState {
        session_id: s.session_id,
        recording: s.recording,
        paused: s.paused,
        elapsed_ms: s.elapsed_ms,
        output: s.output.to_string(),
    })
}
