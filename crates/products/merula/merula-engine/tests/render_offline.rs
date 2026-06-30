//! End-to-end offline-render tests: drive [`render_offline`] over real
//! `Tracks<ControlMap>` and assert the bounce produces a valid, non-silent WAV
//! through the full chain (mixer → bus/send → delay → reverb → limiter). These
//! complement the headless DSP tests in the audio crate's `tests/renderer.rs`
//! (which exercise the mixer/EQ/comp/delay commands directly) by checking that
//! the engine threads the new `VoiceParams` fields and `SetTrackDelay` commands
//! all the way through `schedule` → `render`.

use std::path::PathBuf;

use merula_engine::prelude::{render_offline, RenderConfig};
use merula_pattern::prelude::{pure, seq, track, tracks, ControlMap};

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// A short-tail config so the time-domain convolution reverb doesn't make the
/// bounce slow under a debug build (a long tail is the Onda 3 FFT concern).
fn fast_cfg() -> RenderConfig {
    RenderConfig {
        tail_max_secs: 0.3,
        ..RenderConfig::default()
    }
}

/// Read back a `hound` WAV's peak absolute sample (any channel), for a
/// "did it make sound" assertion.
fn wav_peak(path: &PathBuf) -> f32 {
    let mut reader = hound::WavReader::open(path).expect("open rendered wav");
    let spec = reader.spec();
    match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(Result::ok)
            .fold(0.0f32, |m, s| m.max(s.abs())),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .fold(0.0f32, |m, s| m.max((s as f32 * scale).abs()))
        }
    }
}

#[test]
fn renders_a_drum_pattern_to_audible_wav() {
    // s(bd bd bd bd) → the synth fallback makes sound on each onset.
    let pat = seq(vec![
        pure(ControlMap::sound("bd")),
        pure(ControlMap::sound("bd")),
        pure(ControlMap::sound("bd")),
        pure(ControlMap::sound("bd")),
    ]);
    let t = tracks(vec![track("drums", pat)]);
    let out = temp("merula_render_drums.wav");
    let res = render_offline(&t, 1.0, 1, &fast_cfg(), &out);
    assert!(res.is_ok(), "render should succeed: {res:?}");
    let peak = wav_peak(&out);
    let _ = std::fs::remove_file(&out);
    assert!(peak > 0.001, "rendered drums should be audible, peak={peak}");
}

#[test]
fn renders_with_delay_send_through_the_full_chain() {
    // A note carrying a delay send + line config; the bounce must exercise the
    // per-track delay bus (engine emits SetTrackDelay) and stay valid.
    let mut cm = ControlMap::note(60.0);
    cm.delay = Some(0.125);
    cm.feedback = Some(0.5);
    cm.delay_mix = Some(0.7);
    cm.room = Some(0.4);
    let t = tracks(vec![track("lead", pure(cm))]);
    let out = temp("merula_render_delay.wav");
    let res = render_offline(&t, 1.0, 2, &fast_cfg(), &out);
    assert!(res.is_ok(), "render with delay/reverb should succeed: {res:?}");
    let peak = wav_peak(&out);
    let _ = std::fs::remove_file(&out);
    assert!(peak > 0.001, "delayed/reverbed lead should be audible, peak={peak}");
}

#[test]
fn renders_multiple_tracks_into_one_mix() {
    let drums = pure(ControlMap::sound("bd"));
    let bass = pure(ControlMap::note(36.0));
    let t = tracks(vec![track("drums", drums), track("bass", bass)]);
    let out = temp("merula_render_multitrack.wav");
    let res = render_offline(&t, 1.0, 1, &fast_cfg(), &out);
    assert!(res.is_ok(), "multitrack render should succeed: {res:?}");
    let peak = wav_peak(&out);
    let _ = std::fs::remove_file(&out);
    assert!(peak > 0.001, "multitrack mix should be audible, peak={peak}");
}
