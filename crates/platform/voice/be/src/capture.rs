//! Microphone capture with cpal: mono, resampled to 16 kHz, delivered as chunks over a channel.
//!
//! The `cpal::Stream` is `!Send` on Windows, so it lives on a thread of its own for its whole
//! life. The real-time callback only converts, downmixes, resamples and `send`s — no locks, no
//! allocation beyond the chunk — and everything else happens on the consumer's thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use serde::Serialize;
use voice_core::prelude::{downmix_to_mono, resample_to_16k};

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub default: bool,
}

const NO_MIC: &str = "No microphone was found. Connect one, then pick it in Settings > Voice.";

fn denied_hint(detail: &str) -> String {
    format!(
        "The microphone could not be opened ({detail}). Check that access is allowed: Windows - Settings > \
         Privacy & security > Microphone; macOS - System Settings > Privacy & Security > Microphone; Linux - \
         make sure PulseAudio / PipeWire exposes an input device to this user."
    )
}

pub fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| d.name().ok());
    let devices = host.input_devices().map_err(|e| denied_hint(&e.to_string()))?;
    Ok(devices
        .filter_map(|d| d.name().ok())
        .map(|name| DeviceInfo { default: default.as_deref() == Some(name.as_str()), name })
        .collect())
}

/// A running capture. Dropping it stops the stream and joins the thread.
pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    #[allow(dead_code)] // reported by diagnostics when the microphone misbehaves
    pub device: String,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Open `device` (empty = default input) and start streaming 16 kHz mono chunks.
pub fn start(device: &str) -> Result<(Capture, Receiver<Vec<f32>>), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (chunks_tx, chunks_rx) = mpsc::channel::<Vec<f32>>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
    let wanted = device.to_string();
    let stop_thread = Arc::clone(&stop);

    let thread = thread::Builder::new()
        .name("voice-capture".into())
        .spawn(move || run(&wanted, stop_thread, chunks_tx, ready_tx))
        .map_err(|e| format!("could not start the capture thread: {e}"))?;

    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(name)) => Ok((Capture { stop, thread: Some(thread), device: name }, chunks_rx)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err(denied_hint("the device did not answer in time"))
        }
    }
}

fn pick_device(wanted: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if !wanted.is_empty() {
        let found = host
            .input_devices()
            .map_err(|e| denied_hint(&e.to_string()))?
            .find(|d| d.name().map(|n| n == wanted).unwrap_or(false));
        if let Some(d) = found {
            return Ok(d);
        }
        // The chosen microphone is unplugged: fall back to the default rather than failing.
    }
    host.default_input_device().ok_or_else(|| NO_MIC.to_string())
}

fn run(
    wanted: &str,
    stop: Arc<AtomicBool>,
    chunks: mpsc::Sender<Vec<f32>>,
    ready: mpsc::Sender<Result<String, String>>,
) {
    let built = (|| {
        let device = pick_device(wanted)?;
        let name = device.name().unwrap_or_else(|_| "microphone".into());
        let supported = device.default_input_config().map_err(|e| denied_hint(&e.to_string()))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, chunks),
            SampleFormat::I16 => build::<i16>(&device, &config, chunks),
            SampleFormat::U16 => build::<u16>(&device, &config, chunks),
            other => Err(format!("unsupported microphone sample format {other:?}")),
        }?;
        stream.play().map_err(|e| denied_hint(&e.to_string()))?;
        Ok::<_, String>((stream, name))
    })();

    let (stream, name) = match built {
        Ok(ok) => ok,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(name));
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(40));
    }
    drop(stream);
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    chunks: mpsc::Sender<Vec<f32>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let interleaved: Vec<f32> = data.iter().map(|s| f32::from_sample(*s)).collect();
                let mono = downmix_to_mono(&interleaved, channels);
                let _ = chunks.send(resample_to_16k(&mono, rate));
            },
            |e| eprintln!("voice-be: microphone stream error: {e}"),
            None,
        )
        .map_err(|e| denied_hint(&e.to_string()))
}
