//! Resolve a capture request to a scap [`Target`] and grab a single frame.
//!
//! scap is the one capture backend (WGC / ScreenCaptureKit / PipeWire). A target is
//! just a small Send descriptor (`kind` + `source_id` + optional `region`); the actual
//! scap `Target` (which wraps a non-`Send` OS handle) is re-resolved by its u32 id at
//! grab time — this is the same constraint the streaming producer lives under, so the
//! resolver [`resolve_scap_target`] is shared with [`super::session`].

use std::time::Duration;

use scap::capturer::{Capturer, Options, Resolution};
use scap::frame::{Frame, FrameType};
use scap::Target;

use super::access;

/// A rectangle in physical pixels, display-local, for a region crop.
#[derive(Clone, Copy, Debug)]
pub struct CropRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// A capture request. Send by construction (no OS handles) so it can cross into the
/// grab thread; the scap `Target` is resolved by id there.
pub struct CaptureTarget {
    kind: String,
    source_id: Option<String>,
    region: Option<CropRect>,
}

impl CaptureTarget {
    /// Validate the wire args. `source_id` is `mon-<id>` / `win-<id>`; a region needs
    /// its rectangle (it crops a monitor frame).
    pub fn resolve(kind: &str, source_id: Option<&str>, region: Option<CropRect>) -> Result<Self, String> {
        match kind {
            "monitor" | "window" => {}
            "region" => {
                if region.is_none() {
                    return Err("region capture needs a rectangle".to_string());
                }
            }
            other => return Err(format!("unknown target kind '{other}'")),
        }
        Ok(CaptureTarget { kind: kind.to_string(), source_id: source_id.map(str::to_string), region })
    }

    /// Grab one RGBA frame (bytes, width, height). Runs the scap one-shot on a helper
    /// thread with a hard timeout so a source that never delivers a frame (e.g. a
    /// window that just went minimized) can't freeze the caller. For a region the
    /// monitor frame is cropped to the (physical, display-local) rectangle.
    pub fn grab_rgba(&self) -> Result<(Vec<u8>, u32, u32), String> {
        let kind = self.kind.clone();
        let sid = self.source_id.clone();
        let region = self.region;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = (|| {
                let target = resolve_scap_target(&kind, sid.as_deref())?;
                let (bgra, w, h) = grab_once_bgra(target)?;
                let (bgra, w, h) = if kind == "region" {
                    match region {
                        Some(r) => crop_region_bgra(&bgra, w, h, &r),
                        None => (bgra, w, h),
                    }
                } else {
                    (bgra, w, h)
                };
                if w == 0 || h == 0 {
                    return Err("capture returned a zero-size frame".to_string());
                }
                Ok((bgra_to_rgba(bgra), w, h))
            })();
            let _ = tx.send(res);
        });
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|_| "screen capture timed out".to_string())?
    }
}

/// Resolve a scap `Target` from the wire kind + `source_id`. Shared with the streaming
/// producer (both must re-resolve inside their own thread — the `Target` is not `Send`).
/// `monitor`/`region` map to a Display (by `mon-<id>`, else the first display); `window`
/// maps to a Window by `win-<id>`.
pub fn resolve_scap_target(kind: &str, source_id: Option<&str>) -> Result<Target, String> {
    // Support, permission and scap's habit of panicking on a refusal are all handled
    // once, in `access` — never here.
    let targets = access::targets()?;
    match kind {
        "monitor" | "region" => {
            let want = source_id.and_then(|s| s.strip_prefix("mon-")).and_then(|s| s.parse::<u32>().ok());
            if let Some(id) = want {
                if let Some(t) = targets.iter().find(|t| matches!(t, Target::Display(d) if d.id == id)) {
                    return Ok(t.clone());
                }
            }
            targets
                .into_iter()
                .find(|t| matches!(t, Target::Display(_)))
                .ok_or_else(|| "no monitor available".to_string())
        }
        "window" => {
            let want = source_id
                .and_then(|s| s.strip_prefix("win-"))
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| "window capture needs a source id".to_string())?;
            targets
                .into_iter()
                .find(|t| matches!(t, Target::Window(w) if w.id == want))
                .ok_or_else(|| "the selected window is no longer available".to_string())
        }
        other => Err(format!("unknown target kind '{other}'")),
    }
}

/// One-shot scap capture: build → first frame → stop. Returns raw BGRA + dimensions.
fn grab_once_bgra(target: Target) -> Result<(Vec<u8>, u32, u32), String> {
    access::ensure_permission()?;
    // Building the capturer reaches the same shareable-content call that panics on a
    // refusal, so it is guarded like the enumeration is.
    let mut cap = access::guard("starting the capture", || {
        Capturer::build(Options {
            fps: 60,
            show_cursor: true,
            show_highlight: false,
            target: Some(target),
            crop_area: None,
            output_type: FrameType::BGRAFrame,
            output_resolution: Resolution::Captured,
            excluded_targets: None,
        })
    })?
    .map_err(|e| format!("couldn't start capture: {e:?}"))?;
    cap.start_capture();
    let frame = cap.get_next_frame();
    cap.stop_capture();
    match frame {
        Ok(Frame::BGRA(f)) => Ok((f.data, f.width.max(0) as u32, f.height.max(0) as u32)),
        Ok(_) => Err("capture returned an unexpected frame type".to_string()),
        Err(e) => Err(format!("capture failed: {e}")),
    }
}

/// Crop a `w`×`h` window at `(x, y)` out of a BGRA buffer (4 bytes/px), clamped to the
/// source. Physical, display-local coordinates — scale-agnostic (we crop the captured
/// pixels ourselves rather than using scap's logical `crop_area`).
pub fn crop_region_bgra(src: &[u8], fw: u32, fh: u32, r: &CropRect) -> (Vec<u8>, u32, u32) {
    let x = r.x.max(0) as u32;
    let y = r.y.max(0) as u32;
    let w = r.w.min(fw.saturating_sub(x));
    let h = r.h.min(fh.saturating_sub(y));
    if w == 0 || h == 0 {
        return (Vec::new(), 0, 0);
    }
    let stride = fw as usize * 4;
    let row = w as usize * 4;
    let mut out = Vec::with_capacity(row * h as usize);
    for yy in 0..h as usize {
        let start = (y as usize + yy) * stride + x as usize * 4;
        out.extend_from_slice(&src[start..start + row]);
    }
    (out, w, h)
}

/// Swap B/R in place: scap hands us BGRA, the PNG/`RgbaImage` path wants RGBA.
fn bgra_to_rgba(mut v: Vec<u8>) -> Vec<u8> {
    let mut i = 0;
    while i + 2 < v.len() {
        v.swap(i, i + 2);
        i += 4;
    }
    v
}
