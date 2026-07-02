//! Optional microphone capture to a temp WAV (16-bit PCM), muxed into the final
//! mp4 after recording. Mirrors merula-audio's cpal usage: the `cpal::Stream` is
//! `!Send` on Windows, so it is OWNED on its own thread and never moved. The
//! real-time callback stays light — it converts samples to i16 and hands them to
//! a writer thread over a channel (no file IO in the callback).
//!
//! This is the **microphone** path (cpal input). System audio (render-endpoint
//! loopback) is a separate WASAPI capturer — see [`super::sysaudio`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};

use super::wav::WavWriter;

/// A running mic capture writing to `wav_path`. Drop or [`finalize`](Self::finalize)
/// to stop and flush.
pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub wav_path: PathBuf,
}

impl AudioCapture {
    /// Stop the stream and flush the WAV. Idempotent.
    pub fn finalize(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

/// Start capturing `mic_id` (a cpal device name; `None` = default input) to
/// `wav_path`. Returns once the stream is live, or an error if no device / the
/// stream can't open.
pub fn spawn(mic_id: Option<String>, wav_path: PathBuf) -> Result<AudioCapture, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let path = wav_path.clone();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let thread = thread::spawn(move || {
        run(mic_id, path, stop_thread, ready_tx);
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(AudioCapture { stop, thread: Some(thread), wav_path }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err("audio thread died before signalling readiness".to_string())
        }
    }
}

/// The audio thread body: resolve the device, spawn the WAV writer, build + play
/// the input stream (owned here — `!Send`), signal readiness, park until stop.
fn run(mic_id: Option<String>, wav_path: PathBuf, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    let host = cpal::default_host();
    let device = match resolve_input(&host, mic_id.as_deref()) {
        Some(d) => d,
        None => {
            let _ = ready.send(Err("no microphone / input device available".to_string()));
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("input config: {e}")));
            return;
        }
    };
    let sample_format = supported.sample_format();
    let channels = supported.channels();
    let sample_rate = supported.sample_rate().0;
    let config: cpal::StreamConfig = supported.into();

    // Writer thread: drains i16 chunks to the WAV file, finalizes on channel close.
    let (samp_tx, samp_rx) = mpsc::sync_channel::<Vec<i16>>(64);
    let writer = spawn_writer(samp_rx, wav_path, channels, sample_rate);

    let stream = match build_input(&device, &config, sample_format, samp_tx) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            let _ = writer.join();
            return;
        }
    };
    if let Err(e) = stream.play() {
        let _ = ready.send(Err(format!("start input stream: {e}")));
        drop(stream);
        let _ = writer.join();
        return;
    }
    let _ = ready.send(Ok(()));

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(50));
    }
    // Dropping the stream stops capture and drops the callback's sender, which ends
    // the writer's channel so it finalizes the WAV.
    drop(stream);
    let _ = writer.join();
}

fn resolve_input(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(want) = name {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if d.name().map(|n| n == want).unwrap_or(false) {
                    return Some(d);
                }
            }
        }
    }
    host.default_input_device()
}

fn err_fn(e: cpal::StreamError) {
    eprintln!("tyto-be: audio input stream error: {e}");
}

/// Build an input stream for the device's sample format, converting each sample
/// to i16 and forwarding a chunk per callback. Covers the common formats; an
/// unsupported one errors rather than silently dropping audio.
fn build_input(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    samp_tx: SyncSender<Vec<i16>>,
) -> Result<cpal::Stream, String> {
    macro_rules! build {
        ($t:ty) => {{
            device.build_input_stream(
                config,
                move |data: &[$t], _: &cpal::InputCallbackInfo| {
                    let mut chunk = Vec::with_capacity(data.len());
                    for &s in data {
                        chunk.push(to_i16(s));
                    }
                    // try_send: never block the RT callback; drop under backpressure.
                    let _ = samp_tx.try_send(chunk);
                },
                err_fn,
                None,
            )
        }};
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::U16 => build!(u16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::F64 => build!(f64),
        other => return Err(format!("unsupported input sample format: {other:?}")),
    };
    stream.map_err(|e| format!("build input stream: {e}"))
}

/// Convert any cpal sample to i16 (through f32 so every source format maps).
fn to_i16<T>(s: T) -> i16
where
    T: Sample,
    f32: FromSample<T>,
{
    let f: f32 = f32::from_sample(s);
    (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

// ── WAV sink (16-bit PCM) — see `super::wav::WavWriter` ──────────────────────

fn spawn_writer(rx: Receiver<Vec<i16>>, path: PathBuf, channels: u16, sample_rate: u32) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut w = match WavWriter::create(path, channels, sample_rate) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("tyto-be: WAV create failed: {e}");
                // Drain so senders don't block on a full channel.
                for _ in rx.iter() {}
                return;
            }
        };
        for chunk in rx.iter() {
            for s in chunk {
                if w.write_sample(s).is_err() {
                    break;
                }
            }
        }
        if let Err(e) = w.finalize() {
            eprintln!("tyto-be: WAV finalize failed: {e}");
        }
    })
}
