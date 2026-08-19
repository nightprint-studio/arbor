//! The **video** sink of a recording: exact-cadence frame emission → ffmpeg
//! (libx264 → temp mp4) → mux with the captured audio tracks.
//!
//! Split out of [`super::session`] so the engine reads as "capture, then hand the
//! frames to a sink": this file and [`super::frames`] are the two sinks, and the
//! engine knows nothing about libx264 or about manifests. Everything Tyto knows
//! about ffmpeg lives here.
//!
//! ffmpeg is driven via `ffmpeg-sidecar`'s `FfmpegCommand` using raw `.arg()`s —
//! the arg strings are the stable contract even if the typed builder helpers
//! differ across 2.x.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::audio::{self, AudioCapture};
use super::sysaudio::{self, SysAudioCapture};

/// A live video encode: the emitter, the ffmpeg child, and the audio captures that
/// get muxed in at the end.
pub struct VideoPipeline {
    temp_video: PathBuf,
    emitter: Option<JoinHandle<()>>,
    encoder: Option<JoinHandle<Result<(), String>>>,
    audio: Option<AudioCapture>,
    sys_audio: Option<SysAudioCapture>,
}

/// Everything the engine has resolved for a video recording.
pub struct VideoConfig {
    /// Session id — names the temp files.
    pub id: String,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub mic_id: Option<String>,
    pub system_audio: bool,
}

impl VideoPipeline {
    /// Bring up audio, then the encoder and the frame emitter.
    ///
    /// Audio FIRST on purpose: each `spawn` blocks until its stream is live, so
    /// starting the video after them makes both begin at nearly the same instant
    /// instead of the video leading by the audio spin-up time.
    pub fn start(
        cfg: VideoConfig,
        dims: (u32, u32),
        latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
        stop: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        count: Arc<AtomicU64>,
    ) -> Self {
        let tmp = std::env::temp_dir();
        let id = &cfg.id;
        let temp_video = tmp.join(format!("tyto-{id}.mp4"));

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

        // scap hands us BGRA on every target (monitor / window / region).
        let (w, h) = dims;
        let (tx, rx) = mpsc::sync_channel::<Arc<Vec<u8>>>(8);
        let encoder = spawn_encoder(rx, temp_video.clone(), cfg.fps, w, h, cfg.bitrate_kbps, "bgra");
        let emitter = spawn_emitter(cfg.fps, latest, tx, stop, paused, count);

        VideoPipeline { temp_video, emitter: Some(emitter), encoder: Some(encoder), audio, sys_audio }
    }

    /// Join the emitter + encoder, finalize the audio tracks and mux everything into
    /// `out`. `video_ms` is the recording's pause-excluded length, used only for the
    /// A/V-sync diagnostics.
    pub fn finish(mut self, out: &Path, video_ms: u64) -> Result<(), String> {
        if let Some(h) = self.emitter.take() {
            let _ = h.join(); // stops sampling, drops the frame sender → encoder EOF
        }
        let enc = self.encoder.take().map(|h| h.join()); // ffmpeg drains + finalizes

        // Finalize each audio track (flushes its WAV). Order is irrelevant — they
        // mux into one mixed track.
        let mut audio_paths: Vec<PathBuf> = Vec::new();
        if let Some(cap) = self.audio.take() {
            let p = cap.wav_path.clone();
            cap.finalize();
            audio_paths.push(p);
        }
        if let Some(cap) = self.sys_audio.take() {
            let p = cap.wav_path.clone();
            cap.finalize();
            audio_paths.push(p);
        }
        append_audio_diagnostics(&audio_paths, video_ms);
        // Drop tracks that captured essentially nothing (only the 44-byte WAV header
        // ± a few samples). A dead/silent capture must never bring the recording down
        // to a muxing-only file — better a video with no audio than no video.
        audio_paths.retain(|p| std::fs::metadata(p).map(|m| m.len() > 1024).unwrap_or(false));

        match enc {
            Some(Ok(Ok(()))) | None => {}
            Some(Ok(Err(e))) => return Err(format!("video encode failed: {e}")),
            Some(Err(_)) => return Err("the encoder thread panicked".to_string()),
        }

        mux(&self.temp_video, &audio_paths, out)?;
        let _ = std::fs::remove_file(&self.temp_video);
        for p in &audio_paths {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }
}

/// The **emit** loop: sample `latest` onto the encoder at an EXACT wall-clock fps
/// cadence (duplicating when the producer lags), so frame_count == fps × real seconds
/// and playback time == real time. Arc frames make a duplicate a cheap refcount bump.
///
/// The frame-sequence sink deliberately does the opposite (see [`super::frames`]):
/// duplicates are what a video container needs and what a still sequence must not pay
/// for.
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
