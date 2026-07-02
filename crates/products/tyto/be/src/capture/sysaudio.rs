//! System-audio capture — records "what you hear" (game/app/browser audio),
//! independent of and mixable with the mic, into a temp WAV the session muxes in.
//!
//! **Windows**: render-endpoint loopback via WASAPI (the real impl below).
//! **macOS / Linux**: not implemented yet — [`spawn`] returns an error and the
//! recording proceeds *without* system audio (video + mic still captured). Native
//! capture there (macOS ScreenCaptureKit audio; Linux a PipeWire/PulseAudio monitor
//! source) is a follow-up that needs those platforms to build + test against, so we
//! don't ship an untested loopback rather than guess.
//!
//! HARD-TO-VERIFY (Windows, blast-radius = this file): mirrors the wasapi 0.23
//! loopback idiom — open the **default Render device**, get its `IAudioClient`, and
//! `initialize_client` it with **`Direction::Capture`**; that combination arms the
//! WASAPI loopback flag, and `get_audiocaptureclient` then yields the played frames.
//! We use **polling** shared mode (not events): the render engine keeps delivering
//! silent frames while our stream is active, so a fixed-interval read gives gap-free
//! audio AND a responsive stop (an event handle wouldn't fire during silence).
//! `use wasapi::*` deliberately: a glob import avoids mis-typing individual symbol
//! names on this pin — if the API shifts, the fix stays here.
//!
//! Format: we read the endpoint's REAL mix rate + channel count and request Float32
//! at exactly those (only the sample TYPE is forced — a bit-depth change autoconvert
//! handles), so the WAV header matches the data and the audio plays at the right
//! speed. Each 4-byte `f32` sample is clamped to 16-bit PCM.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

#[cfg(target_os = "windows")]
use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use wasapi::*;

#[cfg(target_os = "windows")]
use super::wav::{f32_to_i16, WavWriter};

/// A running system-audio loopback capture writing to `wav_path`. Drop or
/// [`finalize`](Self::finalize) to stop and flush the WAV. Only ever constructed on
/// Windows; on other platforms [`spawn`] errors before one exists.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct SysAudioCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub wav_path: PathBuf,
}

impl SysAudioCapture {
    /// Stop the loopback stream and flush the WAV. Idempotent.
    pub fn finalize(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SysAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

/// System audio isn't implemented off Windows yet — error out so the caller records
/// without it (video + mic still captured) instead of failing the whole recording.
#[cfg(not(target_os = "windows"))]
pub fn spawn(_wav_path: PathBuf) -> Result<SysAudioCapture, String> {
    Err("system-audio capture isn't supported on this platform yet".to_string())
}

/// Start capturing the default render endpoint (system output) to `wav_path`.
/// Returns once the loopback stream is live, or an error if WASAPI can't open it.
#[cfg(target_os = "windows")]
pub fn spawn(wav_path: PathBuf) -> Result<SysAudioCapture, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let path = wav_path.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let thread = thread::spawn(move || run(path, stop_thread, ready_tx));

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(SysAudioCapture { stop, thread: Some(thread), wav_path }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err("system-audio thread died before signalling readiness".to_string())
        }
    }
}

/// The capture-thread body: init COM (this thread's apartment), open the render
/// endpoint in loopback, then poll frames into the WAV until stopped.
#[cfg(target_os = "windows")]
fn run(wav_path: PathBuf, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    macro_rules! bail {
        ($ready:expr, $e:expr) => {{
            let _ = $ready.send(Err($e));
            return;
        }};
    }

    // Bind (don't discard) the COM init result: if this pin returns a guard, it must
    // stay alive for the whole capture — dropping it would CoUninitialize the thread.
    let _com = initialize_mta();
    if _com.is_err() {
        bail!(ready, "COM initialize (MTA) failed".to_string());
    }

    let enumerator = match DeviceEnumerator::new() {
        Ok(e) => e,
        Err(e) => bail!(ready, format!("audio device enumerator: {e}")),
    };
    let device = match enumerator.get_default_device(&Direction::Render) {
        Ok(d) => d,
        Err(e) => bail!(ready, format!("no default render device: {e}")),
    };
    let mut audio_client = match device.get_iaudioclient() {
        Ok(c) => c,
        Err(e) => bail!(ready, format!("open audio client: {e}")),
    };

    // Match the device's REAL mix rate/channels. Hard-coding 48kHz/stereo made the
    // WAV header lie whenever the endpoint differed (e.g. mono, or 44.1kHz), so the
    // audio played at the wrong speed (the classic "2× fast, out of sync"). We only
    // force Float32 for the sample TYPE — a bit-depth change autoconvert handles
    // cleanly — while keeping the endpoint's own rate + channel count.
    let mix = match audio_client.get_mixformat() {
        Ok(m) => m,
        Err(e) => bail!(ready, format!("query mix format: {e}")),
    };
    let sample_rate = mix.get_samplespersec();
    let channels = mix.get_nchannels();
    let desired = WaveFormat::new(32, 32, &SampleType::Float, sample_rate as usize, channels as usize, None);
    let (_def_period, min_period) = match audio_client.get_device_period() {
        Ok(p) => p,
        Err(e) => bail!(ready, format!("device period: {e}")),
    };
    // Polling + shared + autoconvert: WASAPI resamples the mix to our format and we
    // pace the reads ourselves (see the module note on why not event-driven).
    //
    // Buffer sizing (100-ns units): a GENEROUS loopback buffer (~500 ms) so a
    // scheduling stall — ffmpeg pegging the CPU during a recording — can't overflow it
    // and drop frames before our poll thread next runs. The old `min_period` (a few ms)
    // left almost no slack: that overflow is the residual "some spots play ~2× / audio
    // ends a touch early" after the disk writes were decoupled. Latency is irrelevant
    // here (we don't monitor), so a big buffer is pure resilience. Never below the
    // device's own minimum period.
    const LOOPBACK_BUFFER_HNS: i64 = 500 * 10_000; // 500 ms
    let buffer_hns = min_period.max(LOOPBACK_BUFFER_HNS);
    let mode = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: buffer_hns };
    // Direction::Capture on a RENDER device = loopback.
    if audio_client.initialize_client(&desired, &Direction::Capture, &mode).is_err() {
        // A device may reject the large buffer. Initialize can't be retried on the same
        // client (MSDN: subsequent calls may return ALREADY_INITIALIZED even after a
        // failure), so re-open the client and try the minimum period — system audio then
        // still records (with less overflow slack) rather than dropping out entirely.
        audio_client = match device.get_iaudioclient() {
            Ok(c) => c,
            Err(e) => bail!(ready, format!("re-open audio client: {e}")),
        };
        let fallback = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: min_period };
        if let Err(e) = audio_client.initialize_client(&desired, &Direction::Capture, &fallback) {
            bail!(ready, format!("initialize loopback client: {e}"));
        }
    }
    let capture_client = match audio_client.get_audiocaptureclient() {
        Ok(c) => c,
        Err(e) => bail!(ready, format!("capture client: {e}")),
    };
    if let Err(e) = audio_client.start_stream() {
        bail!(ready, format!("start loopback stream: {e}"));
    }

    let writer = match WavWriter::create(wav_path, channels, sample_rate) {
        Ok(w) => w,
        Err(e) => {
            let _ = audio_client.stop_stream();
            bail!(ready, format!("create system-audio WAV: {e}"));
        }
    };

    // Decouple disk writes from the WASAPI read. During a real recording the disk is
    // busy (ffmpeg writing the video), so writing the WAV inline would stall this poll
    // loop; the loopback buffer then overflows and drops frames — the reported "audio
    // is faster and ends early". The poll loop now only reads + converts (fast, no IO)
    // and hands i16 chunks to a writer thread over an UNBOUNDED channel, so the read
    // is never blocked by disk latency. (The mic path already works this way.)
    let (samp_tx, samp_rx) = mpsc::channel::<Vec<i16>>();
    let writer_thread = thread::spawn(move || {
        let mut w = writer;
        for chunk in samp_rx.iter() {
            for s in chunk {
                if w.write_sample(s).is_err() {
                    break;
                }
            }
        }
        if let Err(e) = w.finalize() {
            eprintln!("tyto-be: system-audio WAV finalize failed: {e}");
        }
    });

    let _ = ready.send(Ok(()));

    // Loopback delivers interleaved Float32 frames as raw bytes; drain in 4-byte
    // (one f32 sample) units, clamp to 16-bit PCM, and hand the chunk to the writer.
    let mut queue: VecDeque<u8> = VecDeque::new();
    'poll: while !stop.load(Ordering::Relaxed) {
        // Drain EVERY packet currently queued, not just one. `read_from_device_to_deque`
        // reads a SINGLE packet per call; under load several accumulate between our
        // wakeups, and reading one-per-tick can never catch up → the buffer overflows
        // and frames are lost (a skip that plays as a brief ~2× speed-up). Loop on the
        // next-packet size so any backlog is flushed in one wake.
        loop {
            match capture_client.get_next_packet_size() {
                Ok(Some(n)) if n > 0 => {
                    if capture_client.read_from_device_to_deque(&mut queue).is_err() {
                        break 'poll;
                    }
                }
                Ok(_) => break,      // no more packets queued right now
                Err(_) => break 'poll,
            }
        }
        if queue.len() >= 4 {
            let mut chunk = Vec::with_capacity(queue.len() / 4);
            while queue.len() >= 4 {
                let b = [
                    queue.pop_front().unwrap(),
                    queue.pop_front().unwrap(),
                    queue.pop_front().unwrap(),
                    queue.pop_front().unwrap(),
                ];
                chunk.push(f32_to_i16(f32::from_le_bytes(b)));
            }
            if samp_tx.send(chunk).is_err() {
                break;
            }
        }
        thread::sleep(Duration::from_millis(8));
    }

    let _ = audio_client.stop_stream();
    drop(samp_tx); // ends the writer's iter → finalize
    let _ = writer_thread.join();
}
