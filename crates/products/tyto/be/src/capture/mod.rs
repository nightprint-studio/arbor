//! The Tyto capture engine — screen/window enumeration, screenshots, and video
//! recording. Lives entirely in `tyto-be` (the native capture deps land here,
//! never in the shell). The domain handlers (`sources` / `session` / `library` /
//! `region`) stay thin RPC wrappers over this module.
//!
//! One capture backend: **scap** (native OS capture — WGC / ScreenCaptureKit /
//! PipeWire) for monitor, window AND region, both streaming and one-shot. Threading
//! (see [`session`]): a producer thread stashes the latest BGRA frame, and a **sink**
//! consumes it. There are two sinks, and the capture side cannot tell them apart:
//! [`video`] emits at an exact fps cadence into ffmpeg (libx264 → mp4, `-pix_fmt
//! bgra`), while [`frames`] samples with deduplication into a pool of image writers
//! and records each survivor's timestamp in a manifest. Optional audio threads capture
//! the mic (cpal, [`audio`]) and/or system output (WASAPI render loopback,
//! [`sysaudio`]) to temp WAVs that a final ffmpeg pass muxes in (mixed when both are
//! on). Source enumeration metadata comes from windows-capture on Windows (see
//! [`source`]); scap's thin enumeration is the fallback elsewhere.

/// The single guarded entry point into scap (permission + panic containment).
pub mod access;
pub mod audio;
/// The frame-sequence sink: a deduplicating sampler + image writer pool.
pub mod frames;
/// GDI monitor grab for the region freeze — Windows-only (no WGC yellow border).
#[cfg(target_os = "windows")]
pub mod gdi;
pub mod library;
/// Minimal MP4 header reading — the duration of a recorded video, without ffprobe.
pub mod mp4;
pub mod screenshot;
pub mod session;
pub mod source;
pub mod sysaudio;
pub mod target;
/// UI Automation element enumeration for the "smart" region pick (empty off Windows).
pub mod uia;
/// The video sink: exact-fps emission → ffmpeg → mp4, with the audio mux.
pub mod video;
pub mod wav;
/// Whole-window + whole-monitor pick targets for the on-screen picker overlay
/// (empty off Windows).
pub mod winpick;

pub use session::{RecordingEngine, RecordingOutput, SessionSnapshot, StartConfig, ENGINE};
pub use target::{CaptureTarget, CropRect};

use std::path::PathBuf;

/// Resolve the capture output directory: the configured dir, else the Windows
/// `%USERPROFILE%\Videos\Tyto`, else a captures dir under tyto's data root.
pub fn output_dir() -> PathBuf {
    let cfg = tyto_core::config::load();
    let dir = cfg.output.dir.trim().to_string();
    if !dir.is_empty() {
        return PathBuf::from(dir);
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join("Videos").join("Tyto");
    }
    arbor_core::prelude::tyto_data_dir().join("captures")
}

/// Round a dimension DOWN to the nearest even number — libx264 (yuv420p) rejects
/// odd width/height.
pub fn even_down(n: u32) -> u32 {
    n & !1
}

/// Crop an RGBA buffer to a top-left `w`×`h` window (used to force even, stable
/// dimensions across frames). Assumes `src_w >= w` and `src_h >= h`.
pub fn crop_rgba(src: &[u8], src_w: u32, w: u32, h: u32) -> Vec<u8> {
    let stride = src_w as usize * 4;
    let row = w as usize * 4;
    let mut out = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        let start = y * stride;
        out.extend_from_slice(&src[start..start + row]);
    }
    out
}

/// Expand a filename template (the `%Y%m%d_%H%M%S` subset the default uses) with
/// the current **UTC** wall clock. Unknown text passes through; if the template
/// yields no time tokens a short random suffix is appended to avoid collisions.
pub fn render_template(template: &str) -> String {
    let (y, mo, d, h, mi, s) = now_utc_parts();
    let mut out = template.to_string();
    let subs = [
        ("%Y", format!("{y:04}")),
        ("%m", format!("{mo:02}")),
        ("%d", format!("{d:02}")),
        ("%H", format!("{h:02}")),
        ("%M", format!("{mi:02}")),
        ("%S", format!("{s:02}")),
    ];
    let had_token = subs.iter().any(|(t, _)| out.contains(t));
    for (t, v) in subs {
        out = out.replace(t, &v);
    }
    if !had_token {
        out.push('_');
        out.push_str(&uuid::Uuid::new_v4().simple().to_string()[..8]);
    }
    out
}

/// Civil (Y, M, D, h, m, s) in UTC from the current system time. Uses Howard
/// Hinnant's days→civil algorithm so we need no date crate. UTC (not local) is a
/// deliberate, documented simplification for filename stamping.
fn now_utc_parts() -> (i64, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = ((rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d, hh, mm, ss)
}
