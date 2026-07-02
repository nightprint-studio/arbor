//! The recording engine: scap capture → ffmpeg encode → (optional) mic mux.
//!
//! One engine for the (single) Tyto window, held as a process [`static@ENGINE`]
//! so the handlers reach it without threading it through `TytoState` (tyto-core
//! stays capture-crate-free). Threads: a capture loop (grabs + crops to a fixed
//! even WxH, paces to fps, backpressured by a bounded channel), an encoder
//! (ffmpeg stdin → temp mp4), a mic capture (temp WAV), and a progress ticker.
//! `stop` joins them and runs the mux.
//!
//! ffmpeg is driven via `ffmpeg-sidecar`'s `FfmpegCommand` using raw `.arg()`s —
//! the arg strings are the stable contract even if the typed builder helpers
//! differ across 2.x (flagged as HARD-TO-VERIFY without compiling).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;

use super::audio::{self, AudioCapture};
use super::sysaudio::{self, SysAudioCapture};
use super::target::{crop_region_bgra, resolve_scap_target, CropRect};
use super::{crop_rgba, even_down, render_template};

/// One live recorder for the Tyto window.
pub static ENGINE: LazyLock<RecordingEngine> = LazyLock::new(RecordingEngine::new);

/// Everything the `start_recording` handler resolves before handing to the engine.
pub struct StartConfig {
    pub target_kind: String,
    pub source_id: Option<String>,
    pub region: Option<CropRect>,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub mic_id: Option<String>,
    /// Capture the render endpoint (system output) alongside the video.
    pub system_audio: bool,
    pub out_dir: PathBuf,
    pub filename_template: String,
    pub target_label: String,
}

/// The `session_state` wire shape (also used to seed the FE after a reconnect).
pub struct SessionSnapshot {
    pub session_id: Option<String>,
    pub recording: bool,
    pub paused: bool,
    pub elapsed_ms: u64,
}

impl SessionSnapshot {
    fn idle() -> Self {
        SessionSnapshot { session_id: None, recording: false, paused: false, elapsed_ms: 0 }
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

struct Active {
    id: String,
    #[allow(dead_code)]
    target_label: String,
    out: PathBuf,
    temp_video: PathBuf,
    dims: (u32, u32),
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
    stats: Arc<CaptureStats>,
    timing: Arc<Timing>,
    producer: Option<JoinHandle<()>>,
    emitter: Option<JoinHandle<()>>,
    encoder: Option<JoinHandle<Result<(), String>>>,
    audio: Option<AudioCapture>,
    sys_audio: Option<SysAudioCapture>,
    progress: Option<JoinHandle<()>>,
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
        let tmp = std::env::temp_dir();
        let temp_video = tmp.join(format!("tyto-{id}.mp4"));
        let out = cfg.out_dir.join(format!("{}.mp4", render_template(&cfg.filename_template)));

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
            cfg.fps,
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
            Err(_) => {
                stop.store(true, Relaxed);
                let _ = producer.join();
                return Err("screen capture didn't start in time".to_string());
            }
        };

        // scap hands us BGRA on every target now (monitor / window / region).
        let pix_fmt = "bgra";

        // Bring up audio FIRST (each blocks until its stream is live), then start the
        // video emitter, so audio and video begin at nearly the same instant — keeps
        // them in A/V sync instead of the video leading by the audio spin-up time.
        let audio = match &cfg.mic_id {
            Some(mic) if !mic.trim().is_empty() => {
                let wav = tmp.join(format!("tyto-{id}-mic.wav"));
                match audio::spawn(Some(mic.clone()), wav) {
                    Ok(a) => Some(a),
                    Err(e) => {
                        eprintln!("tyto-be: mic capture unavailable ({e}) — recording without mic");
                        None
                    }
                }
            }
            _ => None,
        };

        let sys_audio = if cfg.system_audio {
            let wav = tmp.join(format!("tyto-{id}-sys.wav"));
            match sysaudio::spawn(wav) {
                Ok(a) => Some(a),
                Err(e) => {
                    eprintln!("tyto-be: system-audio capture unavailable ({e}) — recording without it");
                    None
                }
            }
        } else {
            None
        };

        // Capture is live and feeding `latest` → wire the encoder + fps emitter.
        let (tx, rx) = mpsc::sync_channel::<Arc<Vec<u8>>>(8);
        let encoder = spawn_encoder(rx, temp_video.clone(), cfg.fps, w, h, cfg.bitrate_kbps, pix_fmt);
        let emitter = spawn_emitter(cfg.fps, Arc::clone(&latest), tx, Arc::clone(&stop), Arc::clone(&paused), Arc::clone(&count));

        let timing = Arc::new(Timing::new());
        let progress = spawn_progress(sink, Arc::clone(&stop), Arc::clone(&timing), Arc::clone(&count), Arc::clone(&lost));

        *guard = Some(Active {
            id: id.clone(),
            target_label: cfg.target_label,
            out,
            temp_video,
            dims: (w, h),
            stop,
            paused,
            count,
            stats,
            timing,
            producer: Some(producer),
            emitter: Some(emitter),
            encoder: Some(encoder),
            audio,
            sys_audio,
            progress: Some(progress),
        });
        Ok(id)
    }

    /// Pause / resume. Video frames stop while paused; the elapsed clock excludes
    /// paused time. (Audio — mic and system — keeps running while paused: a known
    /// v1 A/V-drift limitation.)
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

    /// Stop: signal + join the threads, finalize audio, mux, return the final path.
    pub fn stop(&self) -> Result<PathBuf, String> {
        let active = {
            let mut guard = self.inner.lock().map_err(|_| "engine lock poisoned".to_string())?;
            guard.take()
        };
        let mut a = active.ok_or_else(|| "no recording to stop".to_string())?;

        a.stop.store(true, Relaxed);
        a.paused.store(false, Relaxed);

        if let Some(h) = a.emitter.take() {
            let _ = h.join(); // stops sampling, drops the frame sender → encoder EOF
        }
        if let Some(h) = a.producer.take() {
            let _ = h.join(); // stops capturing
        }
        let enc = a.encoder.take().map(|h| h.join()); // ffmpeg drains + finalizes the video
        if let Some(h) = a.progress.take() {
            let _ = h.join();
        }
        write_capture_diagnostics(&a);
        // Finalize each audio track (flushes its WAV). Order is irrelevant — they
        // mux into one mixed track.
        let mut audio_paths: Vec<PathBuf> = Vec::new();
        if let Some(cap) = a.audio.take() {
            let p = cap.wav_path.clone();
            cap.finalize();
            audio_paths.push(p);
        }
        if let Some(cap) = a.sys_audio.take() {
            let p = cap.wav_path.clone();
            cap.finalize();
            audio_paths.push(p);
        }
        // A/V-sync diagnostics: log each track's real rate/channels/duration next to
        // the video's, so a "2× fast / wrong speed" report shows exactly which stream
        // is off (append to the same temp file as the video diagnostics).
        append_audio_diagnostics(&audio_paths, a.timing.elapsed_ms());
        // Drop tracks that captured essentially nothing (only the 44-byte WAV header
        // ± a few samples). A dead/silent capture must never bring the recording down
        // to a muxing-only file — better a video with no audio than no video.
        audio_paths.retain(|p| std::fs::metadata(p).map(|m| m.len() > 1024).unwrap_or(false));

        match enc {
            Some(Ok(Ok(()))) | None => {}
            Some(Ok(Err(e))) => return Err(format!("video encode failed: {e}")),
            Some(Err(_)) => return Err("the encoder thread panicked".to_string()),
        }

        mux(&a.temp_video, &audio_paths, &a.out)?;
        let _ = std::fs::remove_file(&a.temp_video);
        for p in &audio_paths {
            let _ = std::fs::remove_file(p);
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
                },
                None => SessionSnapshot::idle(),
            },
            Err(_) => SessionSnapshot::idle(),
        }
    }
}

// ── Timing (pause-aware elapsed) ─────────────────────────────────────────────

struct Timing {
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
    fn elapsed_ms(&self) -> u64 {
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

/// The **emit** loop: sample `latest` onto the encoder at an EXACT wall-clock fps
/// cadence (duplicating when the producer lags), so frame_count == fps × real seconds
/// and playback time == real time. Arc frames make a duplicate a cheap refcount bump.
fn spawn_emitter(
    fps: u32,
    latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    tx: SyncSender<Arc<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    count: Arc<AtomicU64>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let frame_dur = Duration::from_nanos(1_000_000_000 / fps.max(1) as u64);
        let mut next = Instant::now() + frame_dur;
        while !stop.load(Relaxed) {
            if paused.load(Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
                next = Instant::now() + frame_dur; // don't burst frames after resume
                continue;
            }
            let snap = latest.lock().ok().and_then(|g| g.clone());
            if let Some(frame) = snap {
                if tx.send(frame).is_err() {
                    break; // encoder gone
                }
                count.fetch_add(1, Relaxed);
            }
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
            next += frame_dur;
            if next < Instant::now() {
                next = Instant::now() + frame_dur; // fell behind — resync, no burst
            }
        }
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

    if !scap::is_supported() {
        let _ = ready.send(Err("screen capture isn't supported on this system".to_string()));
        return;
    }
    if !scap::has_permission() && !scap::request_permission() {
        let _ = ready.send(Err("screen-capture permission was denied".to_string()));
        return;
    }

    let target = match resolve_scap_target(kind, source_id.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let mut capturer = match Capturer::build(Options {
        fps,
        target: Some(target),
        show_cursor: true,
        show_highlight: false,
        excluded_targets: None,
        output_type: FrameType::BGRAFrame,
        output_resolution: Resolution::Captured,
        crop_area: None,
    }) {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("couldn't start screen capture: {e:?}")));
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
/// signal is `unique_fps`: the emitter always hits the target fps (via duplicates),
/// so a low `unique_fps` with a normal `emitted_fps` means the *producer* is the
/// bottleneck (scap is the only producer now).
fn write_capture_diagnostics(a: &Active) {
    let unique = a.stats.stashed.load(Relaxed);
    let emitted = a.count.load(Relaxed);
    let elapsed_ms = a.timing.elapsed_ms().max(1);
    let producer = match a.stats.mode.load(Relaxed) {
        1 => "scap",
        _ => "unknown",
    };
    let unique_fps = unique as f64 * 1000.0 / elapsed_ms as f64;
    let emitted_fps = emitted as f64 * 1000.0 / elapsed_ms as f64;
    let (w, h) = a.dims;
    let line = format!(
        "producer={producer} res={w}x{h} elapsed_ms={elapsed_ms} unique_frames={unique} unique_fps={unique_fps:.1} emitted_frames={emitted} emitted_fps={emitted_fps:.1}\n"
    );
    let path = std::env::temp_dir().join("tyto-last-capture.txt");
    let _ = std::fs::write(&path, line);
}

/// Append per-track audio diagnostics to `%TEMP%/tyto-last-capture.txt`. For each WAV
/// it records the header rate/channels and the duration those imply
/// (`data_bytes / (rate·channels·2)`); comparing that to `video_ms` pinpoints an A/V
/// speed mismatch — e.g. a track whose duration is ~half the video's is being written
/// with a doubled sample rate or channel count (the "2× fast" symptom).
fn append_audio_diagnostics(audio_paths: &[PathBuf], video_ms: u64) {
    use std::io::Write as _;
    let path = std::env::temp_dir().join("tyto-last-capture.txt");
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else { return };
    for p in audio_paths {
        let label = p.file_name().and_then(|n| n.to_str()).unwrap_or("audio");
        match super::wav::read_wav_info(p) {
            Some((rate, ch, data_bytes)) => {
                let denom = (rate as u64) * (ch as u64) * 2;
                let dur_ms = if denom > 0 { data_bytes as u64 * 1000 / denom } else { 0 };
                let _ = writeln!(
                    f,
                    "audio={label} rate={rate} channels={ch} data_bytes={data_bytes} audio_ms={dur_ms} video_ms={video_ms} ratio={:.3}",
                    if video_ms > 0 { dur_ms as f64 / video_ms as f64 } else { 0.0 }
                );
            }
            None => {
                let _ = writeln!(f, "audio={label} (unreadable header)");
            }
        }
    }
}

/// ffmpeg stdin feeder: raw `pix_fmt` frames on `pipe:0` → libx264 → temp mp4.
/// `pix_fmt` is always `bgra` (scap hands us BGRA on every target).
#[allow(clippy::too_many_arguments)]
fn spawn_encoder(
    rx: Receiver<Arc<Vec<u8>>>,
    temp_video: PathBuf,
    fps: u32,
    w: u32,
    h: u32,
    bitrate_kbps: u32,
    pix_fmt: &'static str,
) -> JoinHandle<Result<(), String>> {
    std::thread::spawn(move || {
        use ffmpeg_sidecar::command::FfmpegCommand;
        use std::io::Write;

        let size = format!("{w}x{h}");
        let fps_s = fps.to_string();
        let brate = format!("{bitrate_kbps}k");

        let mut cmd = FfmpegCommand::new();
        cmd.arg("-hide_banner")
            .arg("-loglevel").arg("error")
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg(pix_fmt)
            .arg("-s").arg(&size)
            .arg("-r").arg(&fps_s)
            .arg("-i").arg("pipe:0")
            .arg("-c:v").arg("libx264")
            .arg("-preset").arg("veryfast")
            .arg("-pix_fmt").arg("yuv420p")
            .arg("-b:v").arg(&brate)
            .arg("-y").arg(&temp_video);

        let mut child = cmd.spawn().map_err(|e| format!("ffmpeg spawn: {e}"))?;
        {
            let mut stdin = child.take_stdin().ok_or_else(|| "ffmpeg: no stdin pipe".to_string())?;
            for frame in rx.iter() {
                if stdin.write_all(&frame[..]).is_err() {
                    break;
                }
            }
            // stdin dropped here → ffmpeg sees EOF and finalizes.
        }
        let status = child.wait().map_err(|e| format!("ffmpeg wait: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ffmpeg exited with {status}"))
        }
    })
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

/// Combine the temp video with 0..N audio WAVs into the final mp4. With no audio
/// the video is just moved into place; with one it's muxed as-is; with several they
/// are mixed down (`amix`) into a single AAC track.
fn mux(video: &Path, audio: &[PathBuf], out: &Path) -> Result<(), String> {
    use ffmpeg_sidecar::command::FfmpegCommand;

    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if audio.is_empty() {
        return std::fs::rename(video, out)
            .or_else(|_| std::fs::copy(video, out).map(|_| ()))
            .map_err(|e| format!("move recording to output: {e}"));
    }

    let mut cmd = FfmpegCommand::new();
    cmd.arg("-hide_banner").arg("-loglevel").arg("error");
    cmd.arg("-i").arg(video);
    for a in audio {
        cmd.arg("-i").arg(a);
    }
    cmd.arg("-map").arg("0:v");
    if audio.len() == 1 {
        cmd.arg("-map").arg("1:a");
    } else {
        // Mix every audio input (1:a, 2:a, …) down to a single track. normalize=0
        // keeps levels from being scaled down by the number of inputs.
        let inputs: String = (1..=audio.len()).map(|i| format!("[{i}:a]")).collect();
        let filter = format!("{inputs}amix=inputs={}:duration=longest:normalize=0[aout]", audio.len());
        cmd.arg("-filter_complex").arg(&filter);
        cmd.arg("-map").arg("[aout]");
    }
    // NB: no `-shortest`. The video is the master track; trimming to the shortest
    // stream would collapse the whole clip if an audio track under-ran or came back
    // empty (e.g. a silent WASAPI loopback), which produced ~0-byte files.
    cmd.arg("-c:v").arg("copy")
        .arg("-c:a").arg("aac")
        .arg("-b:a").arg("160k")
        .arg("-y").arg(out);

    let mut child = cmd.spawn().map_err(|e| format!("ffmpeg mux spawn: {e}"))?;
    let status = child.wait().map_err(|e| format!("ffmpeg mux wait: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpeg mux exited with {status}"));
    }
    Ok(())
}
