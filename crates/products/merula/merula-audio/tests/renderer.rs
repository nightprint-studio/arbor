//! Headless DSP tests: build [`VoiceEvent`]s and drive [`Renderer::process`]
//! directly — no audio device, no real time. Assert that the synth fallback
//! makes sound at the right frame, that `gain` scales amplitude, `pan`
//! distributes L/R, and that a one-shot decays toward silence.

use merula_audio::prelude::*;

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
        legato: false,
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
        let p = VoiceParams { gain: 1.0, ..Default::default() };
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, p))], 2048);
        peaks(&out).0
    };
    let quiet = {
        let mut r = Renderer::new(SR, &tracks());
        let p = VoiceParams { gain: 0.25, ..Default::default() };
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
    let left_p = VoiceParams { pan: 0.0, ..Default::default() };
    let mut r = Renderer::new(SR, &tracks());
    let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, left_p))], 1024);
    let (l, rr) = peaks(&out);
    assert!(l > rr * 4.0, "pan=0 should be mostly left: L={l} R={rr}");

    // Hard right.
    let right_p = VoiceParams { pan: 1.0, ..Default::default() };
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
fn stop_all_clears_voices_immediately() {
    let mut r = Renderer::new(SR, &tracks());
    let ev = synth_event(1, 0, VoiceParams::default());
    let _ = render_block(&mut r, vec![AudioCommand::Voice(ev)], 512);
    assert_eq!(r.active_voices(), 1);

    // StopAll is a hard stop: the pool empties in the same block (no ring-out),
    // so the voice count reads zero at once and the renderer is idle.
    let _ = render_block(&mut r, vec![AudioCommand::StopAll], 1);
    assert_eq!(r.active_voices(), 0, "StopAll must drop every voice at once");
}

/// Regression for the "playback keeps running silently in the background, DSP
/// load keeps varying after stop" bug: a `room`/delay tail used to ring on after
/// `StopAll` because the effect buffers were never flushed. `StopAll` must leave
/// the renderer at *exact* silence within one block.
#[test]
fn stop_all_flushes_effect_tails_to_silence() {
    let mut r = Renderer::new(SR, &tracks());

    // A loud voice with a heavy reverb send + a long-feedback per-track delay bus,
    // i.e. exactly the case whose tails outlive the voice.
    let p = VoiceParams { room: 1.0, delay_mix: Some(1.0), ..Default::default() };
    // Configure the track's delay line with strong feedback so, untreated, it would
    // ring for a very long time.
    let cmds = vec![
        AudioCommand::SetTrackDelay(
            0,
            DelayConfig {
                time_frames: 2_400, // 50 ms @ 48 kHz
                feedback: 0.95,
            },
        ),
        AudioCommand::Voice(synth_event(1, 0, p)),
    ];
    let out = render_block(&mut r, cmds, 4_096);
    assert!(peaks(&out).0 > 0.01, "the voice + tails should be audible first");

    // Stop, then render more: with the tails flushed, the output is exact zero —
    // not a slowly-decaying (or, with feedback, non-decaying) ring.
    let _ = render_block(&mut r, vec![AudioCommand::StopAll], 1);
    let tail = render_block(&mut r, vec![], 4_096);
    let (l, rr) = peaks(&tail);
    assert_eq!(
        (l, rr),
        (0.0, 0.0),
        "after StopAll the renderer must be exact silence, got {l},{rr}"
    );
    assert_eq!(r.active_voices(), 0);
}

/// Regression for "the engine stays running after Stop": after `StopAll` (and the
/// voices gone) the renderer must report `is_idle()`, so the real-time callback can
/// skip the whole DSP graph and the footer's DSP load falls to ~0 instead of idling
/// at a ghost ~7%. A fresh renderer is idle too (open-but-unplayed stream), and the
/// next voice must wake it so a Play after Stop renders again.
#[test]
fn stop_all_parks_renderer_idle_and_a_voice_wakes_it() {
    let mut r = Renderer::new(SR, &tracks());
    // Fresh: never played → idle (no DSP work on an open-but-unplayed stream).
    assert!(r.is_idle(), "a fresh renderer must start idle");

    // Play: a voice wakes the DSP path.
    let _ = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, VoiceParams::default()))], 512);
    assert!(!r.is_idle(), "a sounding voice must leave the idle fast-path");

    // Stop: with the pool emptied in the same block, the renderer parks idle.
    let _ = render_block(&mut r, vec![AudioCommand::StopAll], 1);
    assert!(r.is_idle(), "after StopAll (voices gone) the renderer must be idle");

    // A further empty block stays idle and writes exact silence (fast-path).
    let tail = render_block(&mut r, vec![], 1024);
    assert_eq!(peaks(&tail), (0.0, 0.0), "an idle block must be exact silence");
    assert!(r.is_idle(), "still idle with no new voices");

    // Play again (Play after Stop): the next voice must wake the renderer and sound.
    let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(2, 0, VoiceParams::default()))], 1024);
    assert!(!r.is_idle(), "a Play after Stop must wake the renderer");
    assert!(peaks(&out).0 > 0.01, "the restarted voice must be audible");
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

/// Two strips so solo / per-track routing can be exercised.
fn two_tracks() -> Vec<TrackConfig> {
    vec![
        TrackConfig { name: "a".to_string() },
        TrackConfig { name: "b".to_string() },
    ]
}

#[test]
fn master_gain_scales_output() {
    let full = {
        let mut r = Renderer::new(SR, &tracks());
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, VoiceParams::default()))], 2048);
        peaks(&out).0
    };
    let half = {
        let mut r = Renderer::new(SR, &tracks());
        let out = render_block(
            &mut r,
            vec![
                AudioCommand::SetMasterGain(0.25),
                AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())),
            ],
            2048,
        );
        peaks(&out).0
    };
    assert!(half < full * 0.5, "master gain 0.25 should drop the level: {half} vs {full}");
}

#[test]
fn solo_mutes_other_strips() {
    let mut r = Renderer::new(SR, &two_tracks());
    // A voice on each track; solo track 0 → track 1 must be silenced.
    let mut on_b = synth_event(2, 0, VoiceParams::default());
    on_b.track = 1;
    let out = render_block(
        &mut r,
        vec![
            AudioCommand::SetTrackSolo(0, true),
            AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())),
            AudioCommand::Voice(on_b),
        ],
        1024,
    );
    let soloed_peak = peaks(&out).0;
    assert!(soloed_peak > 0.01, "soloed track should sound");

    // Now solo only track 1 and play just track 0 → silence.
    let mut r2 = Renderer::new(SR, &two_tracks());
    let out2 = render_block(
        &mut r2,
        vec![
            AudioCommand::SetTrackSolo(1, true),
            AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())), // track 0
        ],
        1024,
    );
    let (l, rr) = peaks(&out2);
    assert!(l < 1e-6 && rr < 1e-6, "non-soloed track must be silent: {l},{rr}");
}

#[test]
fn track_pan_balances_toward_a_side() {
    // Strip pan to hard-right with a centred voice → right dominates.
    let mut r = Renderer::new(SR, &tracks());
    let out = render_block(
        &mut r,
        vec![
            AudioCommand::SetTrackPan(0, 1.0),
            AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())),
        ],
        1024,
    );
    let (l, rr) = peaks(&out);
    assert!(rr > l * 2.0, "strip pan=1 should bias right: L={l} R={rr}");
}

#[test]
fn lowpass_eq_attenuates_a_bright_synth() {
    use merula_audio::prelude::{EqBand, EqBandKind};
    // A bright saw through a steep low-pass EQ should peak lower than dry.
    let dry = {
        let mut r = Renderer::new(SR, &tracks());
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, VoiceParams::default()))], 2048);
        peaks(&out).0
    };
    let filtered = {
        let mut r = Renderer::new(SR, &tracks());
        let band = EqBand { kind: EqBandKind::Lpf, freq: 200.0, gain_db: 0.0, q: 0.707 };
        let out = render_block(
            &mut r,
            vec![
                AudioCommand::SetTrackEq(0, vec![band]),
                AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())),
            ],
            2048,
        );
        peaks(&out).0
    };
    assert!(filtered < dry, "low-pass EQ should reduce a bright synth's peak: {filtered} vs {dry}");
}

#[test]
fn compressor_reduces_a_loud_strip() {
    use merula_audio::prelude::CompSettings;
    let uncomp = {
        let mut r = Renderer::new(SR, &tracks());
        let out = render_block(&mut r, vec![AudioCommand::Voice(synth_event(1, 0, VoiceParams::default()))], 4096);
        peaks(&out).0
    };
    let comp = {
        let mut r = Renderer::new(SR, &tracks());
        let settings = CompSettings { threshold_db: -30.0, ratio: 8.0, attack: 0.001, release: 0.05, makeup_db: 0.0, knee_db: 2.0 };
        let out = render_block(
            &mut r,
            vec![
                AudioCommand::SetTrackComp(0, Some(settings)),
                AudioCommand::Voice(synth_event(1, 0, VoiceParams::default())),
            ],
            4096,
        );
        peaks(&out).0
    };
    assert!(comp < uncomp, "heavy compression should lower the peak: {comp} vs {uncomp}");
}

#[test]
fn delay_bus_produces_an_echo_after_the_voice() {
    // A short voice with a delay send: after the source decays, the echo must
    // still ring in a later block.
    let mut r = Renderer::new(SR, &tracks());
    // ~10ms line, plenty of feedback, full send.
    let cfg = merula_audio::prelude::DelayConfig { time_frames: 480, feedback: 0.7 };
    let p = VoiceParams { delay_mix: Some(1.0), ..Default::default() };
    let mut ev = synth_event(1, 0, p);
    ev.dur_frames = Some(256); // short source

    let _ = render_block(
        &mut r,
        vec![AudioCommand::SetTrackDelay(0, cfg), AudioCommand::Voice(ev)],
        1024,
    );
    // Render further; the echo should keep producing output for a while.
    let mut echo_peak = 0.0f32;
    for _ in 0..8 {
        let out = render_block(&mut r, vec![], 1024);
        echo_peak = echo_peak.max(peaks(&out).0);
    }
    assert!(echo_peak > 1e-4, "delay bus should ring after the source stops, got {echo_peak}");
}

#[test]
fn reverb_send_makes_a_wet_tail() {
    // A voice with room send into the convolution reverb should leave a wet tail
    // after the dry source has gone.
    let mut r = Renderer::new(SR, &tracks());
    let p = VoiceParams { room: 1.0, ..Default::default() };
    let mut ev = synth_event(1, 0, p);
    ev.dur_frames = Some(128);
    let _ = render_block(&mut r, vec![AudioCommand::Voice(ev)], 512);
    // Let the dry source release, then look for residual wet energy.
    let mut tail = 0.0f32;
    for _ in 0..6 {
        let out = render_block(&mut r, vec![], 2048);
        tail = tail.max(peaks(&out).0);
    }
    assert!(tail > 1e-5, "reverb send should leave a wet tail, got {tail}");
}

#[test]
fn set_reverb_ir_buffer_is_accepted() {
    use merula_audio::prelude::ReverbIr;
    // Installing an explicit IR must not panic and must still render.
    let mut r = Renderer::new(SR, &tracks());
    let ir: Vec<Frame> = (0..64).map(|i| { let g = 0.5f32.powi(i / 8); [g, g] }).collect();
    let p = VoiceParams { room: 0.8, ..Default::default() };
    let out = render_block(
        &mut r,
        vec![
            AudioCommand::SetReverbIr(ReverbIr::Buffer(ir)),
            AudioCommand::Voice(synth_event(1, 0, p)),
        ],
        2048,
    );
    assert!(peaks(&out).0 > 0.0, "render with a custom IR should produce output");
}
