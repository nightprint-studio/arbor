//! Headless DSP tests: build [`VoiceEvent`]s and drive [`Renderer::process`]
//! directly — no audio device, no real time. Assert that the synth fallback
//! makes sound at the right frame, that `gain` scales amplitude, `pan`
//! distributes L/R, and that a one-shot decays toward silence.

use arbor_grove_audio::prelude::*;

const SR: u32 = 48_000;

/// One mixer strip so voices have somewhere to route.
fn tracks() -> Vec<TrackConfig> {
    vec![TrackConfig {
        name: "main".to_string(),
    }]
}

/// A synth voice event (no registry → resolves to the default-synth fallback).
fn synth_event(id: u64, start_frame: u64, params: VoiceParams) -> VoiceEvent {
    VoiceEvent {
        id: VoiceId(id),
        start_frame,
        dur_frames: None,
        source: VoiceSource::Named {
            sound: None,
            variant: None,
            inst: Some("nope.not.resolved".to_string()),
            art: None,
        },
        note: Some(60.0),
        params,
        track: 0,
        span: None,
    }
}

/// Peak absolute amplitude over a block, per channel.
fn peaks(out: &[Frame]) -> (f32, f32) {
    out.iter().fold((0.0, 0.0), |(l, r), f| {
        (l.max(f[0].abs()), r.max(f[1].abs()))
    })
}

/// Render `frames` of silence-then-output by feeding a single command list.
fn render_block(r: &mut Renderer, cmds: Vec<AudioCommand>, frames: usize) -> Vec<Frame> {
    let mut out = vec![[0.0f32; 2]; frames];
    let mut it = cmds.into_iter();
    r.process(&mut it, &mut out);
    out
}

#[test]
fn synth_voice_produces_sound() {
    let mut r = Renderer::new(SR, &tracks());
    let ev = synth_event(1, 0, VoiceParams::default());
    let out = render_block(&mut r, vec![AudioCommand::Voice(ev)], 1024);
    let (l, _) = peaks(&out);
    assert!(l > 0.01, "synth voice should be audible, peak was {l}");
    assert_eq!(r.active_voices(), 1);
}

#[test]
fn voice_starts_at_its_frame() {
    let mut r = Renderer::new(SR, &tracks());
    // Start at frame 512 within a 1024-frame block.
    let ev = synth_event(1, 512, VoiceParams::default());
    let out = render_block(&mut r, vec![AudioCommand::Voice(ev)], 1024);

    let first_half = &out[..512];
    let second_half = &out[512..];
    let (pre, _) = peaks(first_half);
    let (post, _) = peaks(second_half);
    assert!(pre < 1e-6, "must be silent before its start frame, got {pre}");
    assert!(post > 0.01, "must sound after its start frame, got {post}");
}

#[test]
fn gain_scales_amplitude() {
    let loud = {
        let mut r = Renderer::new(SR, &tracks());
        let mut p = VoiceParams::default();
        p.gain = 1.0;
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, p))], 2048);
        peaks(&out).0
    };
    let quiet = {
        let mut r = Renderer::new(SR, &tracks());
        let mut p = VoiceParams::default();
        p.gain = 0.25;
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, p))], 2048);
        peaks(&out).0
    };
    assert!(loud > quiet, "louder gain must peak higher: {loud} vs {quiet}");
    // Roughly 4× (allowing for the limiter / envelope shaping).
    assert!(quiet < loud * 0.5, "0.25 gain should be well below full gain");
}

#[test]
fn pan_distributes_left_right() {
    // Hard left.
    let mut left_p = VoiceParams::default();
    left_p.pan = 0.0;
    let mut r = Renderer::new(SR, &tracks());
    let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, left_p))], 1024);
    let (l, rr) = peaks(&out);
    assert!(l > rr * 4.0, "pan=0 should be mostly left: L={l} R={rr}");

    // Hard right.
    let mut right_p = VoiceParams::default();
    right_p.pan = 1.0;
    let mut r2 = Renderer::new(SR, &tracks());
    let out2 = render_block(&mut r2, vec![AudioCommand::Voice(synth_event(2, 0, right_p))], 1024);
    let (l2, r2p) = peaks(&out2);
    assert!(r2p > l2 * 4.0, "pan=1 should be mostly right: L={l2} R={r2p}");
}

#[test]
fn one_shot_decays_to_silence() {
    let mut r = Renderer::new(SR, &tracks());
    // dur_frames small → releases quickly, then the release ramp rings out.
    let mut ev = synth_event(1, 0, VoiceParams::default());
    ev.dur_frames = Some(256);
    let _ = render_block(&mut r, vec![AudioCommand::Voice(ev)], 256);

    // Keep rendering with no new commands; the voice should release and finish.
    let mut tail_peak = f32::INFINITY;
    for _ in 0..200 {
        let out = render_block(&mut r, vec![], 1024);
        tail_peak = peaks(&out).0;
        if r.active_voices() == 0 {
            break;
        }
    }
    assert_eq!(r.active_voices(), 0, "voice should have rung out");
    assert!(tail_peak < 1e-3, "tail should decay to near silence, got {tail_peak}");
}

#[test]
fn stop_all_releases_voices() {
    let mut r = Renderer::new(SR, &tracks());
    let ev = synth_event(1, 0, VoiceParams::default());
    let _ = render_block(&mut r, vec![AudioCommand::Voice(ev)], 512);
    assert_eq!(r.active_voices(), 1);

    let _ = render_block(&mut r, vec![AudioCommand::StopAll], 1);
    // Let the release ring out.
    for _ in 0..200 {
        let _ = render_block(&mut r, vec![], 1024);
        if r.active_voices() == 0 {
            break;
        }
    }
    assert_eq!(r.active_voices(), 0, "StopAll should release every voice");
}

#[test]
fn track_mute_silences_strip() {
    let mut r = Renderer::new(SR, &tracks());
    let ev = synth_event(1, 0, VoiceParams::default());
    let out = render_block(
        &mut r,
        vec![
            AudioCommand::SetTrackMute(0, true),
            AudioCommand::Voice(ev),
        ],
        1024,
    );
    let (l, rr) = peaks(&out);
    assert!(l < 1e-6 && rr < 1e-6, "muted strip should be silent: {l},{rr}");
}

#[test]
fn voice_stealing_keeps_pool_bounded() {
    let mut r = Renderer::with_capacity(SR, &tracks(), 8);
    // Fire 32 voices into an 8-slot pool across one block.
    let cmds: Vec<AudioCommand> = (0..32)
        .map(|i| AudioCommand::Voice(synth_event(i, i * 4, VoiceParams::default())))
        .collect();
    let _ = render_block(&mut r, cmds, 256);
    assert!(
        r.active_voices() <= 8,
        "pool must never exceed capacity, got {}",
        r.active_voices()
    );
}

#[test]
fn clock_advances_by_block_length() {
    let mut r = Renderer::new(SR, &tracks());
    assert_eq!(r.now_frame(), 0);
    let _ = render_block(&mut r, vec![], 512);
    assert_eq!(r.now_frame(), 512);
    let _ = render_block(&mut r, vec![], 300);
    assert_eq!(r.now_frame(), 812);
}
