//! Spoken-word source synthesis for the live session.
//!
//! A `speech("…")` source carries a [`SpeechSpec`] on its hap. Before the audio
//! thread plays the arrangement, the command layer renders each referenced spec
//! **off the RT thread** into a one-shot sample and registers it under the spec's
//! content-addressed key (`SpeechSpec::registry_key`), so the renderer resolves
//! it through the normal sample path — exactly like a decoded file.
//!
//! Synthesis (SAM is fast; the OS engine less so) is memoised on disk by key so a
//! registry rebuild (triggered by *any* new voice) never re-renders the same
//! line. The cache is a trivial self-describing blob (`u32` rate + `f32` LE
//! samples) — no extra codec dependency.

use std::collections::HashSet;
use std::path::PathBuf;

use merula::prelude::{synthesize_speech_spec, DecodedAudio, Registry, SpeechSpec};

/// Synthesize every referenced spec whose key is in `needed` and register it into
/// `registry`. Called from `build_registry` on the blocking worker — never the RT
/// thread. A spec that produces no audio is skipped (it falls back to the synth).
pub fn register_into(registry: &mut Registry, specs: &[SpeechSpec], needed: &HashSet<String>) {
    for spec in specs {
        let key = spec.registry_key();
        if !needed.contains(&key) {
            continue;
        }
        let audio = synthesize_cached(spec);
        if !audio.samples.is_empty() {
            registry.insert_sample(key, audio);
        }
    }
}

/// Render `spec`, using the on-disk cache when present.
fn synthesize_cached(spec: &SpeechSpec) -> DecodedAudio {
    let path = cache_file(spec);
    if let Some(audio) = read_cache(&path) {
        return audio;
    }
    let audio = synthesize_speech_spec(spec);
    if !audio.samples.is_empty() {
        write_cache(&path, &audio);
    }
    audio
}

/// `<merula-config>\speech-cache` — per-profile (the global data root now holds
/// the shared heavy assets, so this small synth cache stays profile-scoped).
fn cache_dir() -> PathBuf {
    arbor_core::prelude::merula_config_dir().join("speech-cache")
}

/// The cache file for a spec: the hex part of its `"speech:<hex>"` key.
fn cache_file(spec: &SpeechSpec) -> PathBuf {
    let key = spec.registry_key();
    let hex = key.strip_prefix("speech:").unwrap_or(&key);
    cache_dir().join(format!("{hex}.pcmf32"))
}

/// Read a cached buffer (`u32` rate header + `f32` LE samples). `None` on any
/// problem (missing / truncated / unreadable) — the caller re-synthesizes.
fn read_cache(path: &PathBuf) -> Option<DecodedAudio> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 4 || (bytes.len() - 4) % 4 != 0 {
        return None;
    }
    let sample_rate = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let samples = bytes[4..]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Some(DecodedAudio { samples, sample_rate })
}

/// Write a buffer to the cache (best-effort — a failed write just means the next
/// run re-synthesizes; never surfaced).
fn write_cache(path: &PathBuf, audio: &DecodedAudio) {
    let mut bytes = Vec::with_capacity(4 + audio.samples.len() * 4);
    bytes.extend_from_slice(&audio.sample_rate.to_le_bytes());
    for &s in &audio.samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}
