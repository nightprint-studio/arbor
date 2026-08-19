//! Frame-sequence recording: the **same** capture producer as the video path, with
//! the encoder swapped for a pool of image writers plus a manifest.
//!
//! Two things make this not "the video pipeline with PNGs":
//!
//! 1. **A frame is written only when the pixels actually changed.** The video path
//!    has to hit an exact fps cadence (playback time must equal real time), so it
//!    duplicates the last frame whenever the producer lags. Here duplication is pure
//!    cost — a tutorial is a still screen most of the time — so the sampler
//!    deduplicates and the manifest carries each frame's **presentation time**
//!    instead. Ten seconds of a frozen screen is one file, not 120.
//! 2. **The wall clock lives in the manifest, not in the file names.** Index `i`
//!    plays at `times[i]` ms; the player holds a frame until the next one is due.
//!    That is how a GIF works, and it is what the future single-file container
//!    (see the compression proposal) will carry, so nothing has to be re-derived
//!    when the sequence gets folded into one file.
//!
//! On disk a sequence is a **directory** named `<stem>.frames/` holding
//! `frame_000000.<ext>` …, a `poster.png` thumbnail and `manifest.json`. A directory
//! (rather than a bag of loose files in the output dir) keeps the library's
//! "id == file stem" invariant intact: `resolve_path` finds the directory exactly
//! the way it finds an mp4.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::session::Timing;

/// Extension of a frame-sequence **directory** (`tyto_20260819_120000.frames`).
pub const DIR_EXT: &str = "frames";
/// Manifest file inside a sequence directory.
pub const MANIFEST_NAME: &str = "manifest.json";
/// Poster thumbnail inside a sequence directory (the library's list thumbnail).
pub const POSTER_NAME: &str = "poster.png";
/// Widest edge of the poster thumbnail, in pixels.
const POSTER_MAX_W: u32 = 480;
/// Upper bound on writer threads. Past this the disk, not the CPU, is the limit.
const MAX_WORKERS: usize = 4;

/// What a sequence directory says about itself. Written once at stop, read by the
/// library scanner and the player.
///
/// `size_bytes` / `frame_count` are recorded here on purpose: the scanner would
/// otherwise have to `stat` every frame of every sequence on each library refresh,
/// which is thousands of syscalls to answer "how big is it".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version — bumped only on a breaking change to this shape.
    pub version: u32,
    /// Discriminator so a stray `manifest.json` isn't mistaken for a sequence.
    pub kind: String,
    pub width: u32,
    pub height: u32,
    /// Extension of every frame file (`png` | `jpg` | `webp`) — the RESOLVED one,
    /// so it never promises a format this build can't write.
    pub format: String,
    /// The sampling ceiling the recording ran at (the real rate is lower).
    pub sample_fps: u32,
    /// Total length, pause excluded — how long the LAST frame is held for.
    pub duration_ms: u64,
    /// Unix ms at stop.
    pub created_at: i64,
    /// Human label of what was captured.
    pub target: String,
    /// Bytes on disk across every frame (poster and manifest excluded).
    pub size_bytes: u64,
    pub frame_count: usize,
    /// Presentation time of each frame, ms from the start. `times[0]` is always 0.
    pub times: Vec<u32>,
}

/// `kind` value that marks a directory as one of ours.
const MANIFEST_KIND: &str = "tyto-frames";

/// The file holding frame `index`.
pub fn frame_path(dir: &Path, index: usize, ext: &str) -> PathBuf {
    dir.join(format!("frame_{index:06}.{ext}"))
}

/// Read a sequence directory's manifest. Errors when it is missing, unparseable or
/// belongs to something else.
pub fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(dir.join(MANIFEST_NAME))
        .map_err(|e| format!("frame sequence manifest: {e}"))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("frame sequence manifest: {e}"))?;
    if m.kind != MANIFEST_KIND {
        return Err("not a Tyto frame sequence".to_string());
    }
    Ok(m)
}

/// Whether `dir` looks like a frame sequence (named `*.frames` and carrying a
/// manifest). Cheap enough for the library scan — one `exists` per candidate.
pub fn is_sequence_dir(dir: &Path) -> bool {
    dir.is_dir()
        && dir.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case(DIR_EXT)).unwrap_or(false)
        && dir.join(MANIFEST_NAME).is_file()
}

/// Everything the engine resolves before handing the sequence sink its work.
pub struct FramesConfig {
    /// The `<stem>.frames` directory to create and fill.
    pub dir: PathBuf,
    /// Requested frame format (resolved against the compiled encoders).
    pub format: String,
    /// Sampling ceiling in fps.
    pub sample_fps: u32,
    /// Downscale each frame to at most this width (0 = captured resolution).
    pub max_width: u32,
}

// ── The recorder ─────────────────────────────────────────────────────────────

/// A live frame-sequence sink: a sampler thread feeding a pool of writer threads.
pub struct FrameRecorder {
    dir: PathBuf,
    ext: &'static str,
    sample_fps: u32,
    out_dims: (u32, u32),
    pool: WriterPool,
    sampler: Option<JoinHandle<SamplerOutput>>,
}

impl FrameRecorder {
    /// Create the sequence directory and start sampling `latest`.
    ///
    /// `src_dims` are the producer's fixed frame dimensions; the frames written are
    /// those dimensions scaled down to `max_width` when one is set.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        cfg: FramesConfig,
        src_dims: (u32, u32),
        latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
        stop: Arc<std::sync::atomic::AtomicBool>,
        paused: Arc<std::sync::atomic::AtomicBool>,
        count: Arc<AtomicU64>,
        timing: Arc<Timing>,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&cfg.dir).map_err(|e| format!("create frame sequence dir: {e}"))?;
        let ext = super::screenshot::resolve_format(&cfg.format);
        let out_dims = scaled_dims(src_dims, cfg.max_width);

        let workers = worker_count();
        let (pool, tx) = WriterPool::spawn(cfg.dir.clone(), ext, workers);
        let sampler = spawn_sampler(cfg.sample_fps, src_dims, out_dims, latest, tx, stop, paused, count, timing);

        Ok(FrameRecorder {
            dir: cfg.dir,
            ext,
            sample_fps: cfg.sample_fps,
            out_dims,
            pool,
            sampler: Some(sampler),
        })
    }

    /// Join the sampler and the writers, then write the poster + manifest. Returns
    /// the sequence directory.
    ///
    /// Order matters: the sampler is the only sender, so it must be joined before
    /// the pool's channel is dropped, or the last frames are lost.
    pub fn finish(mut self, target: &str, duration_ms: u64) -> Result<PathBuf, String> {
        let out = match self.sampler.take().map(|h| h.join()) {
            Some(Ok(o)) => o,
            Some(Err(_)) => return Err("the frame sampler thread panicked".to_string()),
            None => SamplerOutput::default(),
        };
        let (size_bytes, write_err) = self.pool.finish();

        if out.times.is_empty() {
            // Nothing landed — say so instead of leaving an empty directory that
            // looks like a capture in the library.
            let _ = std::fs::remove_dir_all(&self.dir);
            return Err(match write_err {
                Some(e) => format!("the frame sequence captured no frames: {e}"),
                None => "the frame sequence captured no frames".to_string(),
            });
        }

        // Normalize to a zero-based timeline: the first sample lands a fraction of a
        // frame after the clock starts, and a player that trusts `times` shouldn't
        // open on a blank beat because of it.
        let base = out.times[0];
        let times: Vec<u32> = out.times.iter().map(|t| t.saturating_sub(base)).collect();

        if let Some(first) = out.first.as_ref() {
            write_poster(&self.dir, first, out.src_dims);
        }

        let manifest = Manifest {
            version: 1,
            kind: MANIFEST_KIND.to_string(),
            width: self.out_dims.0,
            height: self.out_dims.1,
            format: self.ext.to_string(),
            sample_fps: self.sample_fps,
            duration_ms: duration_ms.max(*times.last().unwrap_or(&0) as u64),
            created_at: now_unix_ms(),
            target: target.to_string(),
            size_bytes,
            frame_count: times.len(),
            times,
        };
        let text = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join(MANIFEST_NAME), text)
            .map_err(|e| format!("write frame sequence manifest: {e}"))?;

        if let Some(e) = write_err {
            // Some frames failed but the sequence is playable — the diagnostics file
            // is the right place for it, not a failed recording.
            eprintln!("tyto-be: frame sequence had write errors: {e}");
        }
        Ok(self.dir)
    }
}

/// Frames written so far are useful; a diagnostic line about the run is too.
pub fn diagnostics_line(dir: &Path) -> String {
    match read_manifest(dir) {
        Ok(m) => format!(
            "frames dir={} res={}x{} format={} sample_fps={} frames={} bytes={} duration_ms={}\n",
            dir.display(), m.width, m.height, m.format, m.sample_fps, m.frame_count, m.size_bytes, m.duration_ms
        ),
        Err(e) => format!("frames dir={} (manifest unreadable: {e})\n", dir.display()),
    }
}

// ── Sampler ──────────────────────────────────────────────────────────────────

/// What the sampler hands back when it stops.
#[derive(Default)]
struct SamplerOutput {
    /// Raw presentation times (pause-excluded ms), one per enqueued frame.
    times: Vec<u32>,
    /// The first sampled frame, kept for the poster thumbnail.
    first: Option<Arc<Vec<u8>>>,
    /// Dimensions `first` is in.
    src_dims: (u32, u32),
}

/// Sample `latest` at up to `fps`, skipping frames identical to the previous one,
/// and hand each survivor to the writer pool stamped with the pause-aware clock.
///
/// The send is **blocking** by design. The producer keeps overwriting `latest`
/// regardless, so a full queue costs temporal resolution (the next sample happens
/// later) and never a stalled capture — self-pacing, with no dropped changes.
#[allow(clippy::too_many_arguments)]
fn spawn_sampler(
    fps: u32,
    src_dims: (u32, u32),
    out_dims: (u32, u32),
    latest: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
    tx: SyncSender<Job>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    count: Arc<AtomicU64>,
    timing: Arc<Timing>,
) -> JoinHandle<SamplerOutput> {
    std::thread::spawn(move || {
        let period = Duration::from_nanos(1_000_000_000 / fps.max(1) as u64);
        let mut next = Instant::now();
        let mut out = SamplerOutput { times: Vec::new(), first: None, src_dims };
        let mut last_arc: Option<Arc<Vec<u8>>> = None;
        let mut last_hash: Option<u64> = None;

        while !stop.load(Relaxed) {
            if paused.load(Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
                next = Instant::now();
                continue;
            }
            let snap = latest.lock().ok().and_then(|g| g.clone());
            if let Some(frame) = snap {
                // Cheapest rejection first: the producer hasn't delivered anything new
                // since the last sample, so there is nothing to hash.
                let repeat_arc = last_arc.as_ref().is_some_and(|p| Arc::ptr_eq(p, &frame));
                if !repeat_arc {
                    let h = frame_hash(&frame);
                    if last_hash != Some(h) {
                        let index = out.times.len();
                        if index == 0 {
                            out.first = Some(Arc::clone(&frame));
                        }
                        if tx.send(Job { index, bgra: Arc::clone(&frame), src: src_dims, out: out_dims }).is_err() {
                            break; // writers gone
                        }
                        out.times.push(timing.elapsed_ms() as u32);
                        count.fetch_add(1, Relaxed);
                    }
                    last_hash = Some(h);
                    last_arc = Some(frame);
                }
            }
            let now = Instant::now();
            next += period;
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now; // encoding fell behind — resample now, don't burst
            }
        }
        out
    })
}

/// Non-cryptographic 64-bit digest of a frame, used only to answer "did anything
/// change". FNV-1a over 8-byte words plus a splitmix64 finalizer: ~memory-bandwidth
/// fast, which is what makes "don't write a frame that didn't change" cheaper than
/// writing it. A collision costs one skipped frame, never a corrupt file.
fn frame_hash(buf: &[u8]) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunks = buf.chunks_exact(8);
    for c in &mut chunks {
        let w = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        h = (h ^ w).wrapping_mul(PRIME);
    }
    for &b in chunks.remainder() {
        h = (h ^ b as u64).wrapping_mul(PRIME);
    }
    // splitmix64 finalizer — FNV alone leaves the high bits lazy.
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

// ── Writer pool ──────────────────────────────────────────────────────────────

/// One frame to encode and write.
struct Job {
    index: usize,
    bgra: Arc<Vec<u8>>,
    src: (u32, u32),
    out: (u32, u32),
}

/// A fixed set of encode+write threads sharing one bounded queue. Encoding a full
/// desktop frame costs tens of milliseconds, so one thread would cap the real frame
/// rate far below anything usable.
struct WriterPool {
    tx: Option<SyncSender<Job>>,
    workers: Vec<JoinHandle<()>>,
    bytes: Arc<AtomicU64>,
    err: Arc<Mutex<Option<String>>>,
}

impl WriterPool {
    fn spawn(dir: PathBuf, ext: &'static str, workers: usize) -> (Self, SyncSender<Job>) {
        let (tx, rx) = mpsc::sync_channel::<Job>(workers * 2);
        let rx = Arc::new(Mutex::new(rx));
        let bytes = Arc::new(AtomicU64::new(0));
        let err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let handles = (0..workers)
            .map(|_| {
                let rx = Arc::clone(&rx);
                let dir = dir.clone();
                let bytes = Arc::clone(&bytes);
                let err = Arc::clone(&err);
                std::thread::spawn(move || worker_loop(&rx, &dir, ext, &bytes, &err))
            })
            .collect();

        (WriterPool { tx: Some(tx.clone()), workers: handles, bytes, err }, tx)
    }

    /// Drop the queue, join the writers, and report `(bytes written, first error)`.
    fn finish(&mut self) -> (u64, Option<String>) {
        drop(self.tx.take());
        for h in self.workers.drain(..) {
            let _ = h.join();
        }
        let err = self.err.lock().ok().and_then(|g| g.clone());
        (self.bytes.load(Relaxed), err)
    }
}

/// Pull jobs until the queue closes: BGRA → RGBA, optional downscale, encode, write.
///
/// The receiver is shared behind a mutex rather than cloned (an `mpsc` receiver
/// isn't `Sync` on its own); the lock is held only for the `recv`, so the workers
/// contend for a pointer, not for the encode.
fn worker_loop(
    rx: &Arc<Mutex<Receiver<Job>>>,
    dir: &Path,
    ext: &'static str,
    bytes: &Arc<AtomicU64>,
    err: &Arc<Mutex<Option<String>>>,
) {
    loop {
        let job = {
            let guard = match rx.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            match guard.recv() {
                Ok(j) => j,
                Err(_) => return, // channel closed and drained
            }
        };
        let Some(img) = bgra_to_rgba(&job.bgra, job.src.0, job.src.1) else {
            record_err(err, "frame buffer/size mismatch".to_string());
            continue;
        };
        let img = if job.out != job.src {
            image::imageops::thumbnail(&img, job.out.0.max(1), job.out.1.max(1))
        } else {
            img
        };
        let path = frame_path(dir, job.index, ext);
        match super::screenshot::encode_to(&img, &path, ext) {
            Ok(()) => {
                if let Ok(meta) = std::fs::metadata(&path) {
                    bytes.fetch_add(meta.len(), Relaxed);
                }
            }
            Err(e) => record_err(err, e),
        }
    }
}

/// Keep the FIRST error only — the later ones are almost always the same disk
/// saying the same thing, and a hundred repeats teach nothing.
fn record_err(slot: &Arc<Mutex<Option<String>>>, e: String) {
    if let Ok(mut g) = slot.lock() {
        if g.is_none() {
            *g = Some(e);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Writers to run: leave a core for the capture producer, and never more than
/// [`MAX_WORKERS`].
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).clamp(1, MAX_WORKERS))
        .unwrap_or(2)
}

/// `src` scaled so its width is at most `max_width` (0 = unchanged), height kept
/// proportional and never zero.
fn scaled_dims(src: (u32, u32), max_width: u32) -> (u32, u32) {
    let (w, h) = src;
    if max_width == 0 || w <= max_width || w == 0 {
        return (w, h);
    }
    let nh = ((h as u64 * max_width as u64) / w as u64).max(1) as u32;
    (max_width, nh)
}

/// Copy a BGRA capture buffer into an RGBA image (scap hands us BGRA on every
/// target). `None` when the buffer is short for the claimed dimensions.
fn bgra_to_rgba(bgra: &[u8], w: u32, h: u32) -> Option<image::RgbaImage> {
    let need = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    if bgra.len() < need {
        return None;
    }
    let mut buf = bgra[..need].to_vec();
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    image::RgbaImage::from_raw(w, h, buf)
}

/// Write the sequence's poster thumbnail. Best-effort: a missing poster costs the
/// library a thumbnail, never the recording.
fn write_poster(dir: &Path, first: &Arc<Vec<u8>>, src: (u32, u32)) {
    let Some(img) = bgra_to_rgba(first, src.0, src.1) else { return };
    let (w, h) = scaled_dims(src, POSTER_MAX_W);
    let thumb = image::imageops::thumbnail(&img, w.max(1), h.max(1));
    let _ = super::screenshot::encode_to(&thumb, &dir.join(POSTER_NAME), "png");
}

/// Unix milliseconds now.
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_only_shrinks_and_keeps_the_aspect() {
        assert_eq!(scaled_dims((1920, 1080), 0), (1920, 1080), "0 means native");
        assert_eq!(scaled_dims((1920, 1080), 3840), (1920, 1080), "never upscales");
        assert_eq!(scaled_dims((1920, 1080), 960), (960, 540));
        assert_eq!(scaled_dims((100, 3), 10), (10, 1), "height never collapses to 0");
    }

    #[test]
    fn bgra_becomes_rgba_and_rejects_short_buffers() {
        // One pixel: B=1 G=2 R=3 A=4 → R=3 G=2 B=1 A=4.
        let img = bgra_to_rgba(&[1, 2, 3, 4], 1, 1).expect("one pixel");
        assert_eq!(img.get_pixel(0, 0).0, [3, 2, 1, 4]);
        assert!(bgra_to_rgba(&[1, 2, 3], 1, 1).is_none(), "short buffer is rejected");
    }

    #[test]
    fn the_hash_separates_frames_that_differ_by_one_pixel() {
        let a = vec![7u8; 4096];
        let mut b = a.clone();
        b[2048] = 8;
        assert_eq!(frame_hash(&a), frame_hash(&a.clone()), "same bytes, same digest");
        assert_ne!(frame_hash(&a), frame_hash(&b), "one changed byte is visible");
    }

    #[test]
    fn a_sequence_dir_needs_both_the_suffix_and_the_manifest() {
        let root = std::env::temp_dir().join(format!("tyto-frames-test-{}", uuid::Uuid::new_v4().simple()));
        let seq = root.join("clip.frames");
        std::fs::create_dir_all(&seq).unwrap();
        assert!(!is_sequence_dir(&seq), "suffix alone isn't a sequence");
        std::fs::write(seq.join(MANIFEST_NAME), "{}").unwrap();
        assert!(is_sequence_dir(&seq));
        assert!(!is_sequence_dir(&root), "the parent isn't one");
        let _ = std::fs::remove_dir_all(&root);
    }
}
