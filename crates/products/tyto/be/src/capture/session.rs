//! The recording engine: scap capture → **a sink**.
//!
//! One engine for the (single) Tyto window, held as a process [`static@ENGINE`]
//! so the handlers reach it without threading it through `TytoState` (tyto-core
//! stays capture-crate-free).
//!
//! Capture is identical whatever the recording produces: a producer thread grabs
//! frames, crops them to a fixed even WxH and stashes the latest one; a progress
//! thread reports elapsed time and frame count. What differs is the **sink** the
//! frames flow into, and only that:
//!
//! - [`super::video`] — an exact-fps emitter into ffmpeg (libx264 → mp4), with the
//!   mic / system-audio tracks muxed in at stop.
//! - [`super::frames`] — a deduplicating sampler into a pool of image writers,
//!   producing a `<stem>.frames` directory with per-frame timestamps. No audio: a
//!   sequence of stills has nothing to carry it.
//!
//! `stop` joins the shared threads, then hands the sink its own finalization.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;

use super::access;
use super::frames::{FrameRecorder, FramesConfig};
use super::target::{crop_region_bgra, resolve_scap_target, CropRect};
use super::video::{VideoConfig, VideoPipeline};
use super::{crop_rgba, even_down, render_template};

/// One live recorder for the Tyto window.
pub static ENGINE: LazyLock<RecordingEngine> = LazyLock::new(RecordingEngine::new);

/// What a recording produces. The capture side is untouched by this choice — it
/// selects the sink, nothing else.
pub enum RecordingOutput {
    /// H.264 in an mp4, with the captured audio tracks muxed in.
    Video { bitrate_kbps: u32 },
    /// A deduplicated, timestamped image sequence in a `<stem>.frames` directory.
    /// The mic / system-audio settings are ignored here — stills carry no audio.
    Frames { format: String, sample_fps: u32, max_width: u32 },
}

impl RecordingOutput {
    /// The rate the capture source is asked to deliver at. For a frame sequence that
    /// is the sampling ceiling: there is no point having the OS hand us 60 frames a
    /// second when 12 of them will ever be looked at.
    fn capture_fps(&self, cfg_fps: u32) -> u32 {
        match self {
            RecordingOutput::Video { .. } => cfg_fps,
            RecordingOutput::Frames { sample_fps, .. } => (*sample_fps).max(1),
        }
    }
}

/// Everything the `start_recording` handler resolves before handing to the engine.
pub struct StartConfig {
    pub target_kind: String,
    pub source_id: Option<String>,
    pub region: Option<CropRect>,
    pub fps: u32,
    pub mic_id: Option<String>,
    /// Capture the render endpoint (system output) alongside the video.
    pub system_audio: bool,
    pub out_dir: PathBuf,
    pub filename_template: String,
    pub target_label: String,
    /// Video or frame sequence.
    pub output: RecordingOutput,
}

/// The `session_state` wire shape (also used to seed the FE after a reconnect).
pub struct SessionSnapshot {
    pub session_id: Option<String>,
    pub recording: bool,
    pub paused: bool,
    pub elapsed_ms: u64,
    /// `video` | `frames` — what the running session will produce.
    pub output: &'static str,
}

impl SessionSnapshot {
    fn idle() -> Self {
        SessionSnapshot { session_id: None, recording: false, paused: false, elapsed_ms: 0, output: "video" }
    }
}

pub struct RecordingEngine {
    inner: Mutex<Option<Active>>,
}

/// Lightweight capture diagnostics, written to a temp file at stop. Distinguishes a
/// genuinely low encode fps from a low *unique*-frame rate (a slow producer that the
/// emitter has to paper over with duplicates), and reports which producer ran.
struct CaptureStats {
    /// Unique frames the producer actually stashed (≈ real captured fps × seconds).
    stashed: AtomicU64,
    /// 0 = unknown, 1 = scap native capture (the only backend now).
    mode: AtomicU8,
}

/// The live sink of the running session.
enum Sink {
    Video(VideoPipeline),
    Frames(Box<FrameRecorder>),
}

impl Sink {
    fn label(&self) -> &'static str {
        match self {
            Sink::Video(_) => "video",
            Sink::Frames(_) => "frames",
        }
    }
}

struct Active {
    id: String,
    target_label: String,
    /// Final artifact: the mp4 file, or the `.frames` directory.
    out: PathBuf,
    dims: (u32, u32),
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    stats: Arc<CaptureStats>,
    timing: Arc<Timing>,
    producer: Option<JoinHandle<()>>,
    progress: Option<JoinHandle<()>>,
    sink: Option<Sink>,
}

impl RecordingEngine {
    fn new() -> Self {
        RecordingEngine { inner: Mutex::new(None) }
    }

    /// Begin a recording. Errors if one is already running or the target can't be
    /// captured. Returns the session id.
    pub fn start(&self, cfg: StartConfig, sink: Arc<dyn EventSink>) -> Result<String, String> {
        let mut guard = self.inner.lock().map_err(|_| "engine lock poisoned".to_string())?;
        if guard.is_some() {
            return Err("a recording is already in progress".to_string());
        }

        std::fs::create_dir_all(&cfg.out_dir).map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().simple().to_string();
        let stem = render_template(&cfg.filename_template);
        let out = match &cfg.output {
            RecordingOutput::Video { .. } => cfg.out_dir.join(format!("{stem}.mp4")),
            RecordingOutput::Frames { .. } => cfg.out_dir.join(format!("{stem}.{}", super::frames::DIR_EXT)),
        };

        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicU64::new(0));
        let stats = Arc::new(CaptureStats { stashed: AtomicU64::new(0), mode: AtomicU8::new(0) });
        // Set by the producer if the capture source dies *while we still want frames*
        // (monitor unplugged, window closed, capture engine dropped). The progress
        // thread turns it into a one-shot `tyto://recording-error` so the FE can stop
        // and save the partial file instead of freezing on the last frame forever.
        let lost = Arc::new(AtomicBool::new(false));
        let latest: Arc<Mutex<Option<Arc<Vec<u8>>>>> = Arc::new(Mutex::new(None));

        // Start the frame producer and WAIT until it reports the real capture size (its
        // first frame) — or an error. No probe, no silent fallback: if screen capture
        // can't start, the whole recording errors out HERE with a clear message.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u32), String>>();
        let producer = spawn_producer(
            cfg.target_kind.clone(),
            cfg.source_id.clone(),
            cfg.region,
            cfg.output.capture_fps(cfg.fps),
            Arc::clone(&latest),
            Arc::clone(&stop),
            Arc::clone(&paused),
            Arc::clone(&stats),
            Arc::clone(&lost),
            ready_tx,
        );
        let (w, h) = match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(dims)) => dims,
            Ok(Err(e)) => {
                stop.store(true, Relaxed);
                let _ = producer.join();
                return Err(e);
            }
            // Disconnected and timed out are different failures and must not share a
            // message: the producer ending without an answer means it died, and
            // reporting that as "didn't start in time" sends the reader looking for a
            // slow machine instead of for the reason in the log.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop.store(true, Relaxed);
                let _ = producer.join();
                return Err("the screen-capture thread stopped before it produced a frame".to_string());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                stop.store(true, Relaxed);
                let _ = producer.join();
                return Err("screen capture didn't start in time".to_string());
            }
        };

        // The clock starts with the sink, so a frame's timestamp and the elapsed time
        // the HUD shows are the same number.
        let timing = Arc::new(Timing::new());

        // Capture is live and feeding `latest` → wire the chosen sink.
        let sink_impl = match cfg.output {
            RecordingOutput::Video { bitrate_kbps } => Sink::Video(VideoPipeline::start(
                VideoConfig {
                    id: id.clone(),
                    fps: cfg.fps,
                    bitrate_kbps,
                    mic_id: cfg.mic_id.clone(),
                    system_audio: cfg.system_audio,
                },
                (w, h),
                Arc::clone(&latest),
                Arc::clone(&stop),
                Arc::clone(&paused),
                Arc::clone(&count),
            )),
            RecordingOutput::Frames { format, sample_fps, max_width } => {
                let rec = FrameRecorder::start(
                    FramesConfig { dir: out.clone(), format, sample_fps: sample_fps.max(1), max_width },
                    (w, h),
                    Arc::clone(&latest),
                    Arc::clone(&stop),
                    Arc::clone(&paused),
                    Arc::clone(&count),
                    Arc::clone(&timing),
                );
                match rec {
                    Ok(r) => Sink::Frames(Box::new(r)),
                    Err(e) => {
                        stop.store(true, Relaxed);
                        let _ = producer.join();
                        return Err(e);
                    }
                }
            }
        };

        let progress = spawn_progress(sink, Arc::clone(&stop), Arc::clone(&timing), Arc::clone(&count), Arc::clone(&lost));

        *guard = Some(Active {
            id: id.clone(),
            target_label: cfg.target_label,
            out,
            dims: (w, h),
            stop,
            paused,
            count,
            stats,
            timing,
            producer: Some(producer),
            progress: Some(progress),
            sink: Some(sink_impl),
        });
        Ok(id)
    }

    /// Pause / resume. Frames stop while paused; the elapsed clock excludes paused
    /// time, so a frame sequence's timestamps skip the pause too. (Audio — mic and
    /// system — keeps running while paused: a known v1 A/V-drift limitation of the
    /// video sink.)
    pub fn pause(&self, paused: bool) -> Result<(), String> {
        let guard = self.inner.lock().map_err(|_| "engine lock poisoned".to_string())?;
        match &*guard {
            Some(a) => {
                a.paused.store(paused, Relaxed);
                a.timing.set_paused(paused);
                Ok(())
            }
            None => Err("no recording to pause".to_string()),
        }
    }

    /// Stop: signal + join the shared threads, then let the sink finalize itself.
    /// Returns the final path (an mp4 file, or a `.frames` directory).
    pub fn stop(&self) -> Result<PathBuf, String> {
        let active = {
            let mut guard = self.inner.lock().map_err(|_| "engine lock poisoned".to_string())?;
            guard.take()
        };
        let mut a = active.ok_or_else(|| "no recording to stop".to_string())?;

        a.stop.store(true, Relaxed);
        a.paused.store(false, Relaxed);
        // Latch the length ONCE: `Timing` has no stop, so reading it repeatedly during
        // teardown would give each consumer a slightly different recording.
        let elapsed_ms = a.timing.elapsed_ms();

        if let Some(h) = a.producer.take() {
            let _ = h.join(); // stops capturing
        }
        if let Some(h) = a.progress.take() {
            let _ = h.join();
        }
        write_capture_diagnostics(&a, elapsed_ms);

        match a.sink.take() {
            Some(Sink::Video(p)) => p.finish(&a.out, elapsed_ms)?,
            Some(Sink::Frames(r)) => {
                r.finish(&a.target_label, elapsed_ms)?;
                append_diagnostics(&super::frames::diagnostics_line(&a.out));
            }
            None => {}
        }
        Ok(a.out.clone())
    }

    /// Current state (idle when nothing is recording).
    pub fn snapshot(&self) -> SessionSnapshot {
        match self.inner.lock() {
            Ok(guard) => match &*guard {
                Some(a) => SessionSnapshot {
                    session_id: Some(a.id.clone()),
                    recording: true,
                    paused: a.timing.is_paused(),
                    elapsed_ms: a.timing.elapsed_ms(),
                    output: a.sink.as_ref().map(|s| s.label()).unwrap_or("video"),
                },
                None => SessionSnapshot::idle(),
            },
            Err(_) => SessionSnapshot::idle(),
        }
    }
}

// ── Timing (pause-aware elapsed) ─────────────────────────────────────────────

/// The recording's own clock: wall time minus every paused stretch.
///
/// `pub` because the frame sink stamps each frame with it — a sequence's
/// presentation times and the elapsed time the HUD shows have to be the same clock,
/// or a paused recording plays back with a gap nobody recorded.
pub struct Timing {
    start: Instant,
    paused_total_ms: AtomicU64,
    /// 0 = running; else the ms-since-start when the current pause began.
    paused_at_ms: AtomicU64,
}

impl Timing {
    fn new() -> Self {
        Timing { start: Instant::now(), paused_total_ms: AtomicU64::new(0), paused_at_ms: AtomicU64::new(0) }
    }
    fn wall_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    /// Milliseconds of actual recording so far (paused stretches excluded).
    pub fn elapsed_ms(&self) -> u64 {
        let wall = self.wall_ms();
        let paused_total = self.paused_total_ms.load(Relaxed);
        let at = self.paused_at_ms.load(Relaxed);
        let cur = if at != 0 { wall.saturating_sub(at) } else { 0 };
        wall.saturating_sub(paused_total).saturating_sub(cur)
    }
    fn is_paused(&self) -> bool {
        self.paused_at_ms.load(Relaxed) != 0
    }
    fn set_paused(&self, paused: bool) {
        let wall = self.wall_ms();
        if paused {
            if self.paused_at_ms.load(Relaxed) == 0 {
                self.paused_at_ms.store(wall.max(1), Relaxed);
            }
        } else {
            let at = self.paused_at_ms.swap(0, Relaxed);
            if at != 0 {
                self.paused_total_ms.fetch_add(wall.saturating_sub(at), Relaxed);
            }
        }
    }
}

// ── Threads ──────────────────────────────────────────────────────────────────

/// Spawn the frame **producer**: it pushes the latest captured frame into `latest`
/// as fast as scap delivers it and signals `ready` when it's actually capturing (or
/// with an error if it can't). One native path (scap) for monitor / window / region.
///
/// No silent fallback: if capture can't start, the producer reports the error via
/// `ready` and `start` aborts — a clear failure beats a broken clip.
#[allow(clippy::too_many_arguments)]
fn spawn_producer(
    kind: String,
    source_id: Option<String>,
    region: Option<CropRect>,
    fps: u32,
    latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    stats: Arc<CaptureStats>,
    lost: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(u32, u32), String>>,
) -> JoinHandle<()> {
    // The scap Target isn't Send, so the thread gets only the Send descriptor
    // (kind + id + region) and resolves the Target by id INSIDE the thread.
    std::thread::spawn(move || {
        scap_producer(&kind, source_id, region, fps, &latest, &stop, &paused, &stats, &lost, &ready);
    })
}

/// The frame producer via **scap** (native OS capture: WGC / ScreenCaptureKit /
/// PipeWire). Resolves its scap `Target` by id here (the `Target` isn't `Send`), then
/// streams BGRA frames at the requested fps, reporting the real capture size on the
/// first frame (or an error) via `ready`. A **region** captures its display and crops
/// each frame to the (physical, display-local) rectangle in-process — scale-agnostic,
/// no reliance on scap's logical `crop_area`.
///
/// NOTE: `get_next_frame` blocks; WGC only emits frames on change, so on a fully static
/// screen `stop` isn't observed until the next frame. Acceptable — a recording target
/// is virtually never pixel-frozen for the whole session; `stop()` still joins.
#[allow(clippy::too_many_arguments)]
fn scap_producer(
    kind: &str,
    source_id: Option<String>,
    region: Option<CropRect>,
    fps: u32,
    latest: &Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    stop: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    stats: &Arc<CaptureStats>,
    lost: &Arc<AtomicBool>,
    ready: &mpsc::Sender<Result<(u32, u32), String>>,
) {
    use scap::capturer::{Capturer, Options, Resolution};
    use scap::frame::{Frame, FrameType};

    if let Err(e) = access::ensure_permission() {
        let _ = ready.send(Err(e));
        return;
    }

    let target = match resolve_scap_target(kind, source_id.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // Guarded: building the capturer reaches the shareable-content call that panics
    // rather than errors when the OS refuses (see `access`). An unwind here would kill
    // this thread with `ready` never sent, and `start` would be left blaming a timeout.
    let built = access::guard("starting screen capture", || {
        Capturer::build(Options {
            fps,
            target: Some(target),
            show_cursor: true,
            show_highlight: false,
            excluded_targets: None,
            output_type: FrameType::BGRAFrame,
            output_resolution: Resolution::Captured,
            crop_area: None,
        })
    });
    let mut capturer = match built {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            let _ = ready.send(Err(format!("couldn't start screen capture: {e:?}")));
            return;
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    capturer.start_capture();
    stats.mode.store(1, Relaxed); // scap native capture

    // Crop only for a region target; monitor/window use the whole frame.
    let region = if kind == "region" { region } else { None };
    let mut dims: Option<(u32, u32)> = None;
    while !stop.load(Relaxed) {
        match capturer.get_next_frame() {
            Ok(Frame::BGRA(f)) => {
                if paused.load(Relaxed) {
                    continue;
                }
                let (fw, fh) = (f.width.max(0) as u32, f.height.max(0) as u32);
                let (buf, cw, ch) = match &region {
                    Some(r) => crop_region_bgra(&f.data, fw, fh, r),
                    None => (f.data, fw, fh),
                };
                if cw == 0 || ch == 0 {
                    let _ = ready.send(Err("capture returned a zero-size frame".to_string()));
                    break;
                }
                let (w, h) = match dims {
                    Some(d) => d,
                    None => {
                        let d = (even_down(cw), even_down(ch));
                        if d.0 == 0 || d.1 == 0 {
                            let _ = ready.send(Err("capture returned a zero-size frame".to_string()));
                            break;
                        }
                        dims = Some(d);
                        let _ = ready.send(Ok(d));
                        d
                    }
                };
                if let Some(frame) = fit_frame(buf, cw, ch, w, h) {
                    stats.stashed.fetch_add(1, Relaxed);
                    if let Ok(mut g) = latest.lock() {
                        *g = Some(Arc::new(frame));
                    }
                }
            }
            Ok(_) => { /* not the BGRA type we asked for — ignore */ }
            Err(_) => {
                // Capture channel closed. If we didn't ask to stop, the source went
                // away underneath us (monitor unplugged, window closed, resolution/
                // GPU mode switch that dropped the WGC session). Flag it so the FE can
                // stop + save the partial recording instead of freezing forever.
                if !stop.load(Relaxed) {
                    lost.store(true, Relaxed);
                }
                break;
            }
        }
    }
    capturer.stop_capture();
}

/// Force a captured RGBA buffer to the fixed even `w`×`h`: use as-is when it already
/// matches, crop when larger, drop (keep the previous frame) when smaller.
fn fit_frame(raw: Vec<u8>, gw: u32, gh: u32, w: u32, h: u32) -> Option<Vec<u8>> {
    if gw == w && gh == h {
        Some(raw)
    } else if gw >= w && gh >= h {
        Some(crop_rgba(&raw, gw, w, h))
    } else {
        None
    }
}

/// Write a one-line capture diagnostic to `%TEMP%/tyto-last-capture.txt`. The key
/// signal is `unique_fps`: the video emitter always hits the target fps (via
/// duplicates), so a low `unique_fps` with a normal `emitted_fps` means the
/// *producer* is the bottleneck. For a frame sequence `emitted` is the number of
/// frames that survived deduplication, so the two being close means the screen was
/// genuinely changing that often.
fn write_capture_diagnostics(a: &Active, elapsed_ms: u64) {
    let unique = a.stats.stashed.load(Relaxed);
    let emitted = a.count.load(Relaxed);
    let elapsed_ms = elapsed_ms.max(1);
    let producer = match a.stats.mode.load(Relaxed) {
        1 => "scap",
        _ => "unknown",
    };
    let sink = a.sink.as_ref().map(|s| s.label()).unwrap_or("unknown");
    let unique_fps = unique as f64 * 1000.0 / elapsed_ms as f64;
    let emitted_fps = emitted as f64 * 1000.0 / elapsed_ms as f64;
    let (w, h) = a.dims;
    let line = format!(
        "producer={producer} sink={sink} res={w}x{h} elapsed_ms={elapsed_ms} unique_frames={unique} unique_fps={unique_fps:.1} emitted_frames={emitted} emitted_fps={emitted_fps:.1}\n"
    );
    let path = std::env::temp_dir().join("tyto-last-capture.txt");
    let _ = std::fs::write(&path, line);
}

/// Append a line to the same diagnostics file the capture line opened.
pub(super) fn append_diagnostics(line: &str) {
    use std::io::Write as _;
    let path = std::env::temp_dir().join("tyto-last-capture.txt");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Emit `tyto://recording-progress` every 200 ms until stop. Also fires a one-shot
/// `tyto://recording-error` if the producer flags a lost capture source, so the FE
/// can stop and save the partial file rather than record a frozen frame indefinitely.
fn spawn_progress(
    sink: Arc<dyn EventSink>,
    stop: Arc<AtomicBool>,
    timing: Arc<Timing>,
    count: Arc<AtomicU64>,
    lost: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut error_emitted = false;
        while !stop.load(Relaxed) {
            if lost.load(Relaxed) && !error_emitted {
                error_emitted = true;
                sink.emit(
                    "tyto://recording-error",
                    serde_json::json!({
                        "message": "The capture source became unavailable — it may have been disconnected or changed resolution. Saving what was recorded.",
                    }),
                );
            }
            sink.emit(
                "tyto://recording-progress",
                serde_json::json!({
                    "elapsed_ms": timing.elapsed_ms(),
                    "frame_count": count.load(Relaxed),
                }),
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    })
}
