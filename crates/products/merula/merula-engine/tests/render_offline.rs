//! End-to-end offline-render tests: drive [`render_offline`] over real
//! `Tracks<ControlMap>` and assert the bounce produces a valid, non-silent WAV
//! through the full chain (mixer → bus/send → delay → reverb → limiter). These
//! complement the headless DSP tests in the audio crate's `tests/renderer.rs`
//! (which exercise the mixer/EQ/comp/delay commands directly) by checking that
//! the engine threads the new `VoiceParams` fields and `SetTrackDelay` commands
//! all the way through `schedule` → `render`.

use std::path::PathBuf;

use merula_audio::prelude::{DecodedAudio, Registry};
use merula_engine::prelude::{
    render_offline, render_offline_with_registry, warn_unresolved_named_sources, RenderConfig,
};
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

/// Every sample of a rendered WAV as `f32`, for measuring one bounce against
/// another (not just "is it audible").
fn wav_samples(path: &PathBuf) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("open rendered wav");
    let spec = reader.spec();
    match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 * scale)
                .collect()
        }
    }
}

/// Peak + RMS of a sample buffer. Two numbers because neither alone is decisive:
/// a limiter can pin different signals to the same peak, and two very different
/// sounds can land on a similar average level.
fn peak_rms(samples: &[f32]) -> (f32, f32) {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let energy: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (energy / samples.len().max(1) as f64).sqrt() as f32;
    (peak, rms)
}

/// A stand-in for a sample pack's instrument: one second of steady 220 Hz at half
/// scale. Deliberately unlike the fallback triangle synth (which decays through
/// its envelope), so "was the pack loaded" shows up in the measured RMS.
fn pack_sample(sample_rate: u32) -> DecodedAudio {
    let n = sample_rate as usize;
    let samples = (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            0.5 * (std::f32::consts::TAU * 220.0 * t).sin()
        })
        .collect();
    DecodedAudio { samples, sample_rate }
}

/// One track calling a sampled instrument by name.
fn sampled_arrangement(name: &str) -> merula_pattern::prelude::Tracks<ControlMap> {
    let cm = ControlMap {
        inst: Some(name.to_string()),
        ..Default::default()
    };
    tracks(vec![track("mallets", pure(cm))])
}

/// The regression this whole seam exists for: a `.merula` that declares a sampled
/// instrument must render **differently** with its pack loaded than without.
///
/// Before the fix the offline path only ever installed the built-in synths, so
/// both bounces came out byte-identical (the fallback synth twice) and the
/// exported file contradicted its own source. A test that passes in both cases
/// proves nothing — hence the assertion is on measured audio, not on success.
#[test]
fn sampled_instrument_renders_differently_with_its_pack_loaded() {
    let cfg = fast_cfg();
    let arrangement = sampled_arrangement("mallets.hand_chimes");

    // Without the pack: the name resolves to nothing → fallback synth.
    let bare = temp("merula_pack_absent.wav");
    render_offline(&arrangement, 1.0, 1, &cfg, &bare).expect("bare render should succeed");
    let samples_bare = wav_samples(&bare);
    let (peak_bare, rms_bare) = peak_rms(&samples_bare);
    let _ = std::fs::remove_file(&bare);

    // With the pack: same source, same config, only the registry differs.
    let mut registry = Registry::new();
    registry.install_builtin_synths();
    registry.insert_sample("mallets.hand_chimes", pack_sample(cfg.sample_rate));
    let loaded = temp("merula_pack_present.wav");
    render_offline_with_registry(
        &arrangement,
        1.0,
        0,
        1,
        &cfg,
        &loaded,
        registry,
        |_| {},
        || false,
    )
    .expect("pack render should succeed");
    let samples_loaded = wav_samples(&loaded);
    let (peak_loaded, rms_loaded) = peak_rms(&samples_loaded);
    let _ = std::fs::remove_file(&loaded);

    assert!(rms_bare > 0.0, "the fallback bounce should not be silent");
    assert!(rms_loaded > 0.0, "the pack bounce should not be silent");
    // Sample-for-sample identity is the exact shape the defect had: two bounces of
    // the same source, one supposedly with the pack, that were the same file.
    assert!(
        samples_loaded != samples_bare,
        "the two bounces are sample-identical — the render ignored the registry \
         (peak {peak_loaded}, rms {rms_loaded})"
    );
    // …and the difference must be audible, not a rounding artifact.
    let ratio = peak_loaded / peak_bare;
    assert!(
        !(0.9..=1.1).contains(&ratio),
        "loading the pack must change the audio audibly — \
         peak with pack={peak_loaded}, without={peak_bare} (ratio {ratio}); \
         rms {rms_loaded} vs {rms_bare}"
    );
}

/// The guard: an instrument the registry can't resolve is *named*, not silently
/// swapped for the synth. Asserting on the returned list rather than on stderr —
/// same decision, testable.
#[test]
fn unresolved_instrument_is_reported() {
    let known = ControlMap {
        inst: Some("synth.lead".to_string()),
        ..Default::default()
    };
    let arrangement = tracks(vec![
        track("lead", pure(known)),
        track("mallets", pure(ControlMap {
            inst: Some("mallets.hand_chimes".to_string()),
            ..Default::default()
        })),
    ]);

    let mut registry = Registry::new();
    registry.install_builtin_synths();
    assert_eq!(
        warn_unresolved_named_sources(&arrangement, &registry, 0, 1, None),
        vec!["mallets.hand_chimes".to_string()],
        "the unresolved name must be reported, the built-in synth must not"
    );

    // Once the pack is in, nothing is reported.
    registry.insert_sample("mallets.hand_chimes", pack_sample(48_000));
    assert!(
        warn_unresolved_named_sources(&arrangement, &registry, 0, 1, None).is_empty(),
        "a resolvable arrangement must report nothing"
    );
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
