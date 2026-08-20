//! `agent` domain — the handlers that exist for an AI client rather than for the UI.
//!
//! The tyto verbs the frontend uses are shaped for a picker: `take_screenshot` saves a
//! file and returns its path, because the next thing the UI does is show it in the
//! library. A model cannot open a path. It needs the pixels, in its own context, small
//! enough not to cost more than the answer is worth.
//!
//! So this module is not a wrapper around the UI handlers — it is the same capture
//! engine addressed differently: **by window title instead of by opaque id**, and
//! **returning bytes instead of a filename**. Both differences are the same idea, that
//! an agent addresses the world by name and reads it by value.
//!
//! Recording adds a third difference: **it ends by itself**. The UI's
//! `start_recording` returns the moment the engine is live because a human is holding
//! the stop button; a model has no hand on it, so the agent verb takes a duration and
//! drives the same engine to the end, reporting progress on the way and answering with
//! the finished sequence. One engine, two ways of consuming the same run — the pattern
//! `docs/mcp-integration-analysis.md` calls "run e aspetta".

use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use base64::Engine as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tyto_core::prelude::TytoState;

use crate::capture::{self, session::StartConfig, CaptureTarget, RecordingOutput};

/// The long edge a screenshot is scaled down to before it is sent.
///
/// A 4K monitor is 8.8 megapixels; as a PNG in base64 that is several megabytes and,
/// once tokenized, more context than any answer it enables. Vision models resize
/// aggressively on their own side anyway, so sending the full frame buys nothing and
/// costs everything. 1568px is the long edge below which no further downscaling
/// happens for Claude's vision path — the largest size that isn't wasted.
const DEFAULT_MAX_EDGE: u32 = 1568;

/// Hard ceiling on what a caller may ask for, so `max_edge: 100000` can't turn into a
/// multi-megabyte answer by request.
const ABSOLUTE_MAX_EDGE: u32 = 4096;

/// An image handed back inline. The shape `ToolOutput::Image` expects.
#[derive(Debug, Serialize)]
pub struct InlineImage {
    /// Always `image/png` today — lossless, and the alpha survives a freehand mask.
    pub mime_type: String,
    /// Base64, no data-URI prefix.
    pub data: String,
    /// Pixel dimensions of what was actually sent, after downscaling.
    pub width: u32,
    pub height: u32,
    /// The dimensions before downscaling, so a caller can tell what it lost.
    pub source_width: u32,
    pub source_height: u32,
}

/// Args for [`tyto_screenshot`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenshotArgs {
    /// What to capture: `monitor` for a whole display, `window` for one application
    /// window. Defaults to `monitor`.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Which one. For `monitor`, a `mon-<id>` from `tyto_list_sources` — omit for the
    /// primary display. For `window`, either a `win-<id>` or a case-insensitive
    /// fragment of the window title, e.g. "Invoice" or "Visual Studio".
    #[serde(default)]
    pub source: Option<String>,
    /// Scale the image down so its long edge is at most this many pixels. Defaults to
    /// 1568, the point past which detail stops being worth its cost in context.
    #[serde(default)]
    pub max_edge: Option<u32>,
}

/// Capture the screen (or one window) and return the image itself.
///
/// Use this to see what is actually on screen — to read a dialog, check a rendering, or
/// confirm the state of an application that exposes no other interface. For a window,
/// `source` may be part of its title, so there is no need to enumerate first; call
/// `tyto_list_sources` when the title is unknown or ambiguous.
///
/// The image is downscaled (default long edge 1568px) and returned inline as PNG. It is
/// **not** saved to the capture library — nothing appears in the user's recordings.
#[arbor_rpc::handler(mcp(
    name = "tyto_screenshot",
    title = "Capture the screen",
    safety = read,
    output = image,
))]
fn tyto_screenshot(_state: &TytoState, args: ScreenshotArgs) -> Result<InlineImage, String> {
    let kind = args.target_kind.as_deref().unwrap_or("monitor");
    if kind == "region" {
        // A region is a rectangle the user dragged; an agent has no such gesture, and
        // inventing one from coordinates it cannot see would be worse than refusing.
        return Err("region capture needs an interactive selection; capture a monitor or a window instead".into());
    }

    // Resolve a title fragment to a real source id, so the caller can say "Invoice"
    // rather than "win-132498".
    let source_id = match (kind, args.source.as_deref()) {
        ("window", Some(needle)) if !needle.starts_with("win-") => Some(resolve_window(needle)?),
        (_, s) => s.map(str::to_string),
    };

    let target = CaptureTarget::resolve(kind, source_id.as_deref(), None)?;
    let (rgba, width, height) = target.grab_rgba()?;
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "screenshot: buffer/size mismatch".to_string())?;

    let max_edge = args.max_edge.unwrap_or(DEFAULT_MAX_EDGE).clamp(64, ABSOLUTE_MAX_EDGE);
    let scaled = downscale(image, max_edge);
    let (out_w, out_h) = (scaled.width(), scaled.height());

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut std::io::Cursor::new(&mut png))
        .write_image(&scaled, out_w, out_h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("screenshot: png encode failed: {e}"))?;

    Ok(InlineImage {
        mime_type: "image/png".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(&png),
        width: out_w,
        height: out_h,
        source_width: width,
        source_height: height,
    })
}

/// Find the one window whose title contains `needle`, case-insensitively.
///
/// Ambiguity is an error rather than a guess: capturing the wrong window looks like a
/// working call that returned the wrong world, which is the hardest kind of wrong for a
/// model to notice. The error names the candidates so the next call can be exact.
fn resolve_window(needle: &str) -> Result<String, String> {
    let sources = capture::source::list_capture_sources();
    // "No window matches" and "the OS won't let us look" are different answers, and
    // only one of them is about the search term.
    if let Some(reason) = sources.unavailable {
        return Err(reason);
    }
    let needle_lower = needle.to_lowercase();
    let matches: Vec<_> = sources
        .windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&needle_lower))
        .collect();

    match matches.as_slice() {
        [] => Err(format!("no open window's title contains \"{needle}\"")),
        [only] => Ok(only.id.clone()),
        many => {
            let titles: Vec<&str> = many.iter().take(8).map(|w| w.title.as_str()).collect();
            Err(format!(
                "\"{needle}\" matches {} windows ({}). Pass a longer fragment or the exact id.",
                many.len(),
                titles.join(" | ")
            ))
        }
    }
}

/// Scale so the long edge is at most `max_edge`, preserving aspect ratio. An image
/// already within the budget is returned untouched — resampling it would only soften it.
fn downscale(image: image::RgbaImage, max_edge: u32) -> image::RgbaImage {
    let long = image.width().max(image.height());
    if long <= max_edge {
        return image;
    }
    let ratio = max_edge as f32 / long as f32;
    let w = ((image.width() as f32 * ratio).round() as u32).max(1);
    let h = ((image.height() as f32 * ratio).round() as u32).max(1);
    image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle)
}

// ── Frame-sequence recording ─────────────────────────────────────────────────

/// Longest recording a single call may ask for. A tool call that holds a worker for
/// ten minutes is indistinguishable, from the outside, from one that hung.
const MAX_RECORD_SECONDS: u32 = 120;

/// Args for [`tyto_record_frames`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordFramesArgs {
    /// How long to record, in seconds (1-120). The call returns when the recording
    /// has finished, not when it starts.
    pub seconds: u32,
    /// What to capture: `monitor` for a whole display, `window` for one application
    /// window. Defaults to `monitor`.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Which one. For `monitor`, a `mon-<id>` from `tyto_list_sources` — omit for the
    /// primary display. For `window`, either a `win-<id>` or a case-insensitive
    /// fragment of the window title.
    #[serde(default)]
    pub source: Option<String>,
    /// Frames sampled per second, at most. Fewer are written whenever the screen
    /// does not change. Defaults to the user's configured value.
    #[serde(default)]
    pub sample_fps: Option<u32>,
    /// Downscale every frame so its width is at most this many pixels. Omit for the
    /// captured resolution.
    #[serde(default)]
    pub max_width: Option<u32>,
    /// Frame image format: `png` (lossless, the default), `jpg` or `webp`.
    #[serde(default)]
    pub format: Option<String>,
}

/// What a finished frame-sequence recording is.
#[derive(Debug, Serialize)]
pub struct FrameSequenceSummary {
    /// The capture's id in the library — pass it to `tyto_read_frame`.
    pub name: String,
    /// The `.frames` directory on disk.
    pub dir: String,
    /// Frames actually written. Lower than `seconds × sample_fps` by however much
    /// the screen stood still: identical frames are never stored.
    pub frame_count: usize,
    /// Length of the sequence in milliseconds.
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    /// Extension of each frame file.
    pub format: String,
    /// Bytes on disk across every frame.
    pub size_bytes: u64,
}

/// Record the screen for a fixed number of seconds as a **sequence of still images**
/// rather than a video, and return where it landed.
///
/// Use this to capture a short demonstration that has to stay pixel-exact — a UI walk
/// through, a reproduction of a visual bug, the frames behind an animated tutorial.
/// Video compression softens text and thin lines; these frames do not, and each one
/// carries the moment it belongs to, so the sequence plays back at real speed even
/// though nothing was written while the screen stood still.
///
/// The call blocks for the requested duration and the recording is saved to the user's
/// capture library, visible in Tyto like any other. It fails rather than interfering if
/// a recording is already running — check `tyto_recording_state` first when unsure.
#[arbor_rpc::handler(mcp(
    name = "tyto_record_frames",
    title = "Record the screen as an image sequence",
    safety = write,
))]
fn tyto_record_frames(state: &TytoState, args: RecordFramesArgs) -> Result<FrameSequenceSummary, String> {
    let kind = args.target_kind.as_deref().unwrap_or("monitor");
    if kind == "region" {
        return Err("region capture needs an interactive selection; record a monitor or a window instead".into());
    }
    let seconds = args.seconds.clamp(1, MAX_RECORD_SECONDS);
    let source_id = match (kind, args.source.as_deref()) {
        ("window", Some(needle)) if !needle.starts_with("win-") => Some(resolve_window(needle)?),
        (_, s) => s.map(str::to_string),
    };

    let cfg = tyto_core::config::load();
    let sample_fps = args.sample_fps.unwrap_or(cfg.frames.sample_fps).clamp(1, 60);
    let start = StartConfig {
        target_kind: kind.to_string(),
        source_id: source_id.clone(),
        region: None,
        fps: sample_fps,
        // A still sequence carries no audio, so neither is even brought up.
        mic_id: None,
        system_audio: false,
        out_dir: capture::output_dir(),
        filename_template: cfg.output.filename_template.clone(),
        target_label: source_id.unwrap_or_else(|| kind.to_string()),
        output: RecordingOutput::Frames {
            format: args.format.unwrap_or(cfg.frames.format),
            sample_fps,
            max_width: args.max_width.unwrap_or(cfg.frames.max_width),
        },
    };

    let sink = state.event_sink();
    capture::ENGINE.start(start, sink.clone())?;
    drive_recording(sink.as_ref(), seconds);
    let dir = capture::ENGINE.stop()?;

    let m = capture::frames::read_manifest(&dir)?;
    Ok(FrameSequenceSummary {
        name: dir.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        dir: dir.to_string_lossy().to_string(),
        frame_count: m.frame_count,
        duration_ms: m.duration_ms,
        width: m.width,
        height: m.height,
        format: m.format,
        size_bytes: m.size_bytes,
    })
}

/// Sleep out the recording, narrating it on `arbor://progress`.
///
/// The narration is the point: the caller is blocked for up to two minutes, and a
/// tool that says nothing for that long is one the client cannot distinguish from a
/// hang. The Tyto window is filling its own panel from the very same run.
fn drive_recording(sink: &dyn EventSink, seconds: u32) {
    for elapsed in 1..=seconds {
        std::thread::sleep(Duration::from_secs(1));
        sink.progress(
            &format!("Recording frames — {elapsed}s of {seconds}s"),
            Some(elapsed as u64),
            Some(seconds as u64),
        );
    }
}

/// Args for [`tyto_read_frame`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFrameArgs {
    /// The sequence's name in the library — the `name` a recording returned, or the
    /// capture's file stem.
    pub sequence: String,
    /// Which frame, 0-based. Ignored when `at_ms` is given; both omitted = the first.
    #[serde(default)]
    pub index: Option<usize>,
    /// Or pick by time: the frame visible this many milliseconds into the sequence.
    #[serde(default)]
    pub at_ms: Option<u32>,
    /// Scale the image down so its long edge is at most this many pixels. Defaults to
    /// 1568, the point past which detail stops being worth its cost in context.
    #[serde(default)]
    pub max_edge: Option<u32>,
}

/// Read one frame out of a recorded sequence and return the image itself.
///
/// Use it to check what a recording actually captured — that the right window was in
/// frame, that a transition happened when it was supposed to — without opening
/// anything. Address a frame by index, or by the moment it belongs to with `at_ms`,
/// which picks the frame that was on screen then.
#[arbor_rpc::handler(mcp(
    name = "tyto_read_frame",
    title = "Read one frame of a recorded sequence",
    safety = read,
    output = image,
))]
fn tyto_read_frame(_state: &TytoState, args: ReadFrameArgs) -> Result<InlineImage, String> {
    let dir = capture::library::resolve_sequence(&capture::output_dir(), &args.sequence)?;
    let m = capture::frames::read_manifest(&dir)?;
    if m.frame_count == 0 {
        return Err(format!("\"{}\" has no frames", args.sequence));
    }
    let index = match (args.index, args.at_ms) {
        (_, Some(at)) => frame_at(&m.times, at),
        (Some(i), None) => {
            if i >= m.frame_count {
                return Err(format!(
                    "frame {i} is past the end of \"{}\" ({} frames, {} ms)",
                    args.sequence, m.frame_count, m.duration_ms
                ));
            }
            i
        }
        (None, None) => 0,
    };

    let path = capture::frames::frame_path(&dir, index, &m.format);
    let image = image::open(&path).map_err(|e| format!("read frame {index}: {e}"))?.to_rgba8();
    let (source_width, source_height) = (image.width(), image.height());
    let max_edge = args.max_edge.unwrap_or(DEFAULT_MAX_EDGE).clamp(64, ABSOLUTE_MAX_EDGE);
    let scaled = downscale(image, max_edge);
    let (out_w, out_h) = (scaled.width(), scaled.height());

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut std::io::Cursor::new(&mut png))
        .write_image(&scaled, out_w, out_h, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("frame: png encode failed: {e}"))?;

    Ok(InlineImage {
        mime_type: "image/png".to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(&png),
        width: out_w,
        height: out_h,
        source_width,
        source_height,
    })
}

/// The index of the frame **on screen** at `at_ms`: the last one whose presentation
/// time has already passed. A time past the end resolves to the final frame, which is
/// what was still showing.
fn frame_at(times: &[u32], at_ms: u32) -> usize {
    match times.binary_search(&at_ms) {
        Ok(i) => i,
        Err(0) => 0,
        Err(i) => i - 1,
    }
}

// ── Export: a sequence → a sprite atlas ──────────────────────────────────────

/// Arguments of `tyto_export_atlas`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportAtlasArgs {
    /// The recording to fold, as it appears in the library (the name
    /// `tyto_record_frames` returned).
    pub sequence: String,
    /// Where to write the atlas. Omit to write an `<id>.atlas` directory next to the
    /// recording.
    #[serde(default)]
    pub out_dir: Option<String>,
    /// Widest edge of one atlas page, in pixels. Bigger pages mean fewer files and
    /// more memory while the atlas is assembled. Default 4096, hard ceiling 8192 —
    /// the size no mainstream GPU will exceed.
    #[serde(default)]
    pub max_side: Option<u32>,
    /// Name the sequence takes inside the sheet's timeline. Default `play`.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the sequence loops. Default true.
    #[serde(default)]
    pub looping: Option<bool>,
    /// Pixels of guard around each frame, filled by repeating its border, so a renderer
    /// that filters the texture cannot sample a neighbouring frame's pixels along an
    /// edge. Default 1; set 0 for a tight pack when the atlas is only ever drawn 1:1.
    #[serde(default)]
    pub gutter: Option<u32>,
}

/// Where an exported atlas landed.
#[derive(Debug, Serialize)]
pub struct AtlasExportSummary {
    /// The directory holding the pages and the sheet.
    pub dir: String,
    /// The sidecar that describes the atlas.
    pub sheet: String,
    /// The page PNGs, in index order.
    pub pages: Vec<String>,
    pub frame_count: usize,
    /// How many frames fit on one full page.
    pub frames_per_page: usize,
    pub frame_width: u32,
    pub frame_height: u32,
    pub duration_ms: u64,
    /// Bytes written across pages and sheet.
    pub size_bytes: u64,
}

/// Fold a recorded frame sequence into a **sprite atlas**: one or more PNG pages plus
/// an `atlas.ron` sheet that says where every frame sits and how long it is held.
///
/// Use this to hand a recording to a game engine or any renderer that draws from a
/// texture: a directory of several hundred PNGs is one texture upload per frame, an
/// atlas is one upload and a UV lookup. The sheet carries each frame's real duration,
/// so playback keeps the original timing even though identical frames were never
/// stored — a still screen costs one frame, not thirty a second.
///
/// Frames are packed into pages of at most `max_side` pixels per edge; a recording too
/// long for one page spills into `atlas_001.png`, `atlas_002.png` and so on, and every
/// region records which page it belongs to. Nothing is re-encoded lossily: the pages
/// are PNG regardless of the format the frames were recorded in.
#[arbor_rpc::handler(mcp(
    name = "tyto_export_atlas",
    title = "Export a recording as a sprite atlas",
    safety = write,
))]
fn tyto_export_atlas(_state: &TytoState, args: ExportAtlasArgs) -> Result<AtlasExportSummary, String> {
    let root = capture::output_dir();
    let dir = capture::library::resolve_sequence(&root, &args.sequence)?;
    let out = match args.out_dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => std::path::PathBuf::from(p),
        // Next to the recording, under a sibling name: the atlas is a second artefact
        // of the same capture, and burying it inside the `.frames` directory would
        // make the library scanner count its pages as frames.
        None => dir.with_extension("atlas"),
    };

    let opts = capture::atlas::AtlasOptions {
        max_side: args.max_side.unwrap_or(capture::atlas::DEFAULT_MAX_SIDE),
        sequence: args
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("play")
            .to_string(),
        looping: args.looping.unwrap_or(true),
        gutter: args.gutter.unwrap_or(capture::atlas::DEFAULT_GUTTER),
    };

    let report = capture::atlas::export(&dir, &out, &opts)?;
    Ok(AtlasExportSummary {
        dir: report.dir.to_string_lossy().to_string(),
        sheet: report.sheet.to_string_lossy().to_string(),
        pages: report.pages.iter().map(|p| p.to_string_lossy().to_string()).collect(),
        frame_count: report.frame_count,
        frames_per_page: report.per_page,
        frame_width: report.frame_width,
        frame_height: report.frame_height,
        duration_ms: report.duration_ms,
        size_bytes: report.size_bytes,
    })
}

// `write_image` lives on this trait.
use image::ImageEncoder as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_only_shrinks() {
        let small = image::RgbaImage::new(100, 50);
        let out = downscale(small, 1568);
        assert_eq!((out.width(), out.height()), (100, 50), "already inside the budget");

        let big = image::RgbaImage::new(3840, 2160);
        let out = downscale(big, 1568);
        assert_eq!(out.width(), 1568);
        // Aspect ratio held: 2160 * (1568/3840) = 882.
        assert_eq!(out.height(), 882);
    }

    #[test]
    fn a_frame_is_picked_by_the_moment_it_was_on_screen() {
        // A sequence that changed at 0, 500 and 1400 ms.
        let times = [0u32, 500, 1400];
        assert_eq!(frame_at(&times, 0), 0);
        assert_eq!(frame_at(&times, 499), 0, "still the first frame");
        assert_eq!(frame_at(&times, 500), 1, "exactly on a change");
        assert_eq!(frame_at(&times, 1399), 1, "a long-held frame answers for its whole span");
        assert_eq!(frame_at(&times, 9_000), 2, "past the end is the last frame, which was still up");
    }

    #[test]
    fn a_tall_image_is_bounded_by_its_long_edge() {
        let tall = image::RgbaImage::new(500, 2000);
        let out = downscale(tall, 1000);
        assert_eq!(out.height(), 1000);
        assert_eq!(out.width(), 250);
    }
}
