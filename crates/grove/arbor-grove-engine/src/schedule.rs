//! The look-ahead scheduling core — a **pure** function from patterns + clock to
//! sample-accurate trigger events. Shared by the real-time transport
//! ([`crate::transport`]) and the offline render driver ([`crate::render`]); the
//! only thing that differs downstream is whether the events go to a live
//! [`AudioSink`](arbor_grove_audio::prelude::AudioSink) or straight into a
//! [`Renderer`](arbor_grove_audio::prelude::Renderer).
//!
//! No I/O, no audio, no real time → trivially unit-testable.
//!
//! ## Query window
//!
//! [`schedule_span`] is handed a half-open **frame** range `[frames.start,
//! frames.end)`. It converts that to a cycle-time window via [`Epoch::cycle_of`],
//! widens it by a guard cycle on each side (so an onset whose exact frame lands in
//! the range is never missed because of the float `cycle_of` ↔ exact-rational
//! mismatch), queries the patterns, then **re-filters** every emitted event back
//! to `start_frame ∈ [frames.start, frames.end)`. This makes adjacent windows
//! seamless: a hap on the seam is emitted by exactly one window, never both and
//! never neither.
//!
//! ## Sustained dedup
//!
//! A [`VoiceSource::File`] with [`SourceKind::Sustained`] is produced by `pure`,
//! so the pattern places it once **per cycle**. The audio engine plays it once and
//! lets it ring, so the per-cycle repeats are spurious. [`schedule_span`] collapses
//! them *within a single call* unconditionally. Suppressing them *across* calls
//! (so a sustained stem started in window N is not retriggered in window N+1) needs
//! cross-call state, which — because this function is pure — the caller threads in:
//! [`Transport`](crate::transport::Transport) keeps a started-set and filters the
//! returned events through it; [`render_offline`](crate::render::render_offline)
//! does the same with a fresh set per render. The key is `(track, path)`.

use std::collections::HashSet;
use std::ops::Range;

use arbor_grove_audio::prelude::{VoiceEvent, VoiceParams, VoiceSource};
use arbor_grove_pattern::prelude::{ControlMap, Hap, SourceKind, Tracks};

use crate::clock::Epoch;

/// Query `tracks` over the cycle window covered by the `frames` range and emit a
/// [`VoiceEvent`] per onset, sample-accurately placed via `epoch`.
///
/// `next_id` is the running voice-id counter (advanced once per emitted event).
/// Only haps with an onset whose `start_frame` lands in `[frames.start,
/// frames.end)` become events; continuous signals and tail fragments are dropped.
/// A `Sustained` file source emits only on its first onset *within this call*
/// (see the module docs for cross-call dedup, which the caller owns).
///
/// Pure: no I/O, no audio, no real time.
pub fn schedule_span(
    tracks: &Tracks<ControlMap>,
    epoch: &Epoch,
    sample_rate: u32,
    frames: Range<u64>,
    next_id: &mut u64,
) -> Vec<VoiceEvent> {
    if frames.start >= frames.end || tracks.tracks.is_empty() {
        return Vec::new();
    }

    let query = frame_range_to_query_span(epoch, sample_rate, &frames);
    let mut out = Vec::new();
    // Within-call dedup of per-cycle Sustained repeats: a stem starting at the
    // first onset suppresses its own later cycles inside this same window.
    let mut sustained_seen: HashSet<(u32, String)> = HashSet::new();

    for (track_idx, t) in tracks.tracks.iter().enumerate() {
        let track = track_idx as u32;
        let mut haps = t.pattern.query(query);
        // Stable order so id assignment is deterministic and seam-stable: by
        // onset time first, then by source span if present.
        haps.sort_by(|a, b| {
            a.onset()
                .cmp(&b.onset())
                .then_with(|| hap_span_key(a).cmp(&hap_span_key(b)))
        });

        for hap in &haps {
            // Seam re-filter on the UNCLAMPED onset frame: `frame_of` clamps
            // negatives to 0, so the negative guard cycle would otherwise leak in
            // at frame 0 (double-emitting every onset). Keep only onsets whose
            // frame lands in this window, then build the event.
            let onset_frame = epoch.frame_of_signed(hap.onset(), sample_rate);
            if onset_frame < frames.start as i64 || onset_frame >= frames.end as i64 {
                continue;
            }
            let Some(ev) = voice_event_from_hap(hap, track, epoch, sample_rate, *next_id) else {
                continue;
            };
            // Collapse per-cycle Sustained repeats inside this call.
            if let VoiceSource::File {
                path,
                kind: SourceKind::Sustained,
            } = &ev.source
            {
                if !sustained_seen.insert((track, path.clone())) {
                    continue;
                }
            }
            *next_id += 1;
            out.push(ev);
        }
    }

    out
}

/// Sort key pulling a hap's source-span start out for a stable tiebreak; `u32::MAX`
/// sorts span-less haps after spanned ones at the same onset.
fn hap_span_key(hap: &Hap<ControlMap>) -> u32 {
    hap.span.map_or(u32::MAX, |s| s.start)
}

/// Convert the look-ahead **frame** range into the exact-`Time` cycle window to
/// query, widened by a guard cycle on each side.
///
/// `cycle_of` is a lossy `f64` view; an onset's exact frame is recovered by
/// `frame_of`, and the caller re-filters by `start_frame`, so the only job here is
/// to make the query window a strict superset of the cycles that can contain an
/// in-range onset. The guard cycle absorbs the float slop at the seam.
fn frame_range_to_query_span(
    epoch: &Epoch,
    sample_rate: u32,
    frames: &Range<u64>,
) -> arbor_grove_pattern::prelude::TimeSpan {
    use arbor_grove_pattern::prelude::{Time, TimeSpan};

    let begin_cycle = epoch.cycle_of(frames.start, sample_rate);
    let end_cycle = epoch.cycle_of(frames.end, sample_rate);
    // Floor/ceil to whole cycles and pad by one so no boundary onset is missed.
    let begin = (begin_cycle.floor() as i64) - 1;
    let end = (end_cycle.ceil() as i64) + 1;
    TimeSpan::new(Time::int(begin), Time::int(end))
}

/// Map a single onset hap on `track` to a [`VoiceEvent`]: resolve the
/// `ControlMap` into a [`VoiceSource`](arbor_grove_audio::prelude::VoiceSource) +
/// [`VoiceParams`](arbor_grove_audio::prelude::VoiceParams), pitch, and the
/// cycle→frame `start_frame`/`dur_frames`. `None` if the hap is not a playable
/// onset. Exposed for focused unit tests of the mapping.
pub fn voice_event_from_hap(
    hap: &Hap<ControlMap>,
    track: u32,
    epoch: &Epoch,
    sample_rate: u32,
    id: u64,
) -> Option<VoiceEvent> {
    // Continuous signals (no `whole`) and tail fragments are not playable onsets.
    if !hap.has_onset() {
        return None;
    }
    let whole = hap.whole?;
    let v = &hap.value;

    let source = resolve_source(v);
    let note = resolve_note(v);

    let start_frame = epoch.frame_of(hap.onset(), sample_rate);

    // Lifetime from the hap's `whole` × frames-per-cycle. A sustained file source
    // rings to its natural end → `None` (it is one continuous take, not a slot).
    let dur_frames = match &source {
        VoiceSource::File {
            kind: SourceKind::Sustained,
            ..
        } => None,
        _ => {
            let dur = (whole.end - whole.begin).to_f64() * epoch.frames_per_cycle(sample_rate);
            Some(dur.round().max(0.0) as u64)
        }
    };

    let params = resolve_params(v);

    Some(VoiceEvent {
        id: arbor_grove_audio::prelude::VoiceId(id),
        start_frame,
        dur_frames,
        source,
        note,
        params,
        track,
        span: hap.span.map(|s| (s.start, s.end)),
    })
}

/// Resolve the symbolic source: a user file marker wins over a named sound/inst.
fn resolve_source(v: &ControlMap) -> VoiceSource {
    if let Some(path) = &v.source_file {
        VoiceSource::File {
            path: path.clone(),
            kind: v.source_kind.unwrap_or(SourceKind::OneShot),
        }
    } else {
        VoiceSource::Named {
            sound: v.sound.clone(),
            variant: v.variant,
            inst: v.inst.clone(),
            art: v.art.clone(),
        }
    }
}

/// Resolve pitch: an explicit `note` wins; else a raw `degree` (no `scale()`) is
/// taken as a chromatic semitone above middle C — a best-effort fallback for what
/// is technically a user error; else native pitch (`None`).
fn resolve_note(v: &ControlMap) -> Option<f32> {
    if let Some(n) = v.note {
        Some(n as f32)
    } else {
        v.degree.map(|d| 60.0 + d as f32)
    }
}

/// Overlay the `ControlMap` numeric controls onto [`VoiceParams::default`],
/// narrowing `f64 → f32`. Optional effects (`lpf`/`hpf`/`crush`) stay `None`
/// when unset.
fn resolve_params(v: &ControlMap) -> VoiceParams {
    let mut p = VoiceParams::default();
    if let Some(x) = v.gain {
        p.gain = x as f32;
    }
    if let Some(x) = v.pan {
        p.pan = x as f32;
    }
    if let Some(x) = v.room {
        p.room = x as f32;
    }
    if let Some(x) = v.lpf {
        p.lpf = Some(x as f32);
    }
    if let Some(x) = v.hpf {
        p.hpf = Some(x as f32);
    }
    if let Some(x) = v.shift {
        p.shift = x as f32;
    }
    if let Some(x) = v.speed {
        p.speed = x as f32;
    }
    if let Some(x) = v.crush {
        p.crush = Some(x as f32);
    }
    if let Some(x) = v.shape {
        p.shape = x as f32;
    }
    if let Some(x) = v.vel {
        p.vel = x as f32;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_grove_pattern::prelude::{
        audio, pure, seq, track, tracks, Hap, SourceSpan, Time, TimeSpan,
    };

    /// A single discrete hap covering `[begin, end)` of cycle 0, carrying `v`.
    fn hap(v: ControlMap, begin: Time, end: Time) -> Hap<ControlMap> {
        let w = TimeSpan::new(begin, end);
        Hap::new(Some(w), w, v)
    }

    #[test]
    fn maps_named_sound_with_default_params() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let h = hap(ControlMap::sound("bd"), Time::ZERO, Time::ONE);
        let ev = voice_event_from_hap(&h, 0, &e, sr, 7).expect("playable onset");
        assert_eq!(ev.id, arbor_grove_audio::prelude::VoiceId(7));
        assert_eq!(ev.start_frame, 0);
        assert_eq!(ev.dur_frames, Some(48_000));
        assert_eq!(ev.track, 0);
        assert_eq!(ev.note, None);
        assert_eq!(
            ev.source,
            VoiceSource::Named {
                sound: Some("bd".into()),
                variant: None,
                inst: None,
                art: None,
            }
        );
        assert_eq!(ev.params, VoiceParams::default());
    }

    #[test]
    fn second_slot_lands_at_half_cycle_frame() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        // hap at [1/2, 1) of cycle 0 → starts at frame 24_000, lasts 24_000.
        let h = hap(ControlMap::sound("sn"), Time::new(1, 2), Time::ONE);
        let ev = voice_event_from_hap(&h, 1, &e, sr, 0).unwrap();
        assert_eq!(ev.start_frame, 24_000);
        assert_eq!(ev.dur_frames, Some(24_000));
        assert_eq!(ev.track, 1);
    }

    #[test]
    fn continuous_and_tail_haps_drop() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        // Continuous: no `whole`.
        let cont = Hap::new(None, TimeSpan::new(Time::ZERO, Time::ONE), ControlMap::sound("x"));
        assert!(voice_event_from_hap(&cont, 0, &e, sr, 0).is_none());
        // Tail fragment: part begins after whole begins.
        let w = TimeSpan::new(Time::ZERO, Time::ONE);
        let tail = Hap::new(Some(w), TimeSpan::new(Time::new(1, 2), Time::ONE), ControlMap::sound("x"));
        assert!(voice_event_from_hap(&tail, 0, &e, sr, 0).is_none());
    }

    #[test]
    fn note_and_degree_pitch_resolution() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let n = hap(ControlMap::note(64.0), Time::ZERO, Time::ONE);
        assert_eq!(voice_event_from_hap(&n, 0, &e, sr, 0).unwrap().note, Some(64.0));
        let d = hap(ControlMap::degree(3), Time::ZERO, Time::ONE);
        assert_eq!(voice_event_from_hap(&d, 0, &e, sr, 0).unwrap().note, Some(63.0));
    }

    #[test]
    fn params_overlay_narrows_f64_to_f32() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut cm = ControlMap::sound("bd");
        cm.gain = Some(0.5);
        cm.pan = Some(0.25);
        cm.lpf = Some(800.0);
        cm.vel = Some(1.0);
        let ev = voice_event_from_hap(&hap(cm, Time::ZERO, Time::ONE), 0, &e, sr, 0).unwrap();
        assert_eq!(ev.params.gain, 0.5);
        assert_eq!(ev.params.pan, 0.25);
        assert_eq!(ev.params.lpf, Some(800.0));
        assert_eq!(ev.params.vel, 1.0);
        assert_eq!(ev.params.hpf, None); // unset stays None
    }

    #[test]
    fn file_source_one_shot_vs_sustained() {
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut one = ControlMap::source_file("kick.wav");
        one.source_kind = Some(SourceKind::OneShot);
        let ev = voice_event_from_hap(&hap(one, Time::ZERO, Time::ONE), 0, &e, sr, 0).unwrap();
        assert_eq!(
            ev.source,
            VoiceSource::File { path: "kick.wav".into(), kind: SourceKind::OneShot }
        );
        assert_eq!(ev.dur_frames, Some(48_000));

        let mut sus = ControlMap::source_file("pad.wav");
        sus.source_kind = Some(SourceKind::Sustained);
        let ev = voice_event_from_hap(&hap(sus, Time::ZERO, Time::ONE), 0, &e, sr, 0).unwrap();
        assert_eq!(ev.dur_frames, None); // rings to natural end
    }

    #[test]
    fn schedule_span_places_four_onsets_in_a_cycle() {
        // s(bd bd bd bd) on one track over cycle 0.
        let pat = seq(vec![
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
        ]);
        let t = tracks(vec![track("drums", pat)]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..48_000, &mut id);
        assert_eq!(evs.len(), 4);
        let starts: Vec<u64> = evs.iter().map(|v| v.start_frame).collect();
        assert_eq!(starts, vec![0, 12_000, 24_000, 36_000]);
        assert_eq!(id, 4);
        assert!(evs.iter().all(|v| v.track == 0));
    }

    #[test]
    fn schedule_span_reanchored_window_excludes_out_of_range_onsets() {
        let pat = seq(vec![pure(ControlMap::sound("bd")), pure(ControlMap::sound("sn"))]);
        let t = tracks(vec![track("d", pat)]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        // Only the first half of cycle 0: catches the "bd" at 0, not the "sn" at 24_000.
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..24_000, &mut id);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].start_frame, 0);
        assert_eq!(
            evs[0].source,
            VoiceSource::Named { sound: Some("bd".into()), variant: None, inst: None, art: None }
        );
    }

    #[test]
    fn schedule_span_no_double_emit_at_seam() {
        let pat = seq(vec![pure(ControlMap::sound("bd")), pure(ControlMap::sound("sn"))]);
        let t = tracks(vec![track("d", pat)]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        // Two adjacent windows covering the whole cycle, split at the "sn" onset.
        let a = schedule_span(&t, &e, sr, 0..24_000, &mut id);
        let b = schedule_span(&t, &e, sr, 24_000..48_000, &mut id);
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].start_frame, 0);
        assert_eq!(b[0].start_frame, 24_000);
        assert_eq!(id, 2); // exactly two events total, no duplicate at the seam
    }

    #[test]
    fn schedule_span_dedups_sustained_within_window() {
        // `audio(...)` is a Sustained pure → one hap per cycle; over 3 cycles only
        // the first onset survives.
        let t = tracks(vec![track("stem", audio("pad.wav"))]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..(48_000 * 3), &mut id);
        let sustained: Vec<&VoiceEvent> = evs
            .iter()
            .filter(|v| matches!(v.source, VoiceSource::File { kind: SourceKind::Sustained, .. }))
            .collect();
        assert_eq!(sustained.len(), 1, "per-cycle Sustained repeats collapse");
        assert_eq!(sustained[0].start_frame, 0);
    }

    #[test]
    fn schedule_span_carries_source_span() {
        let pat = pure(ControlMap::sound("bd")).tag_span(SourceSpan::new(2, 4));
        let t = tracks(vec![track("d", pat)]);
        let e = Epoch::start(1.0);
        let mut id = 0;
        let evs = schedule_span(&t, &e, 48_000, 0..48_000, &mut id);
        assert_eq!(evs[0].span, Some((2, 4)));
    }

    #[test]
    fn schedule_span_window_in_later_cycle_has_no_leak() {
        // 4 onsets/cycle; querying exactly cycle 1's frames must yield only cycle
        // 1's four onsets — none leaking from the guard cycles 0 or 2 (regression
        // for the negative-guard-cycle clamp-to-0 leak).
        let pat = seq(vec![
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
        ]);
        let t = tracks(vec![track("drums", pat)]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 48_000..96_000, &mut id);
        let starts: Vec<u64> = evs.iter().map(|v| v.start_frame).collect();
        assert_eq!(starts, vec![48_000, 60_000, 72_000, 84_000]);
        assert_eq!(id, 4);
    }

    #[test]
    fn schedule_span_spans_multiple_cycles() {
        // One onset per cycle over three cycles → three events at the cycle starts.
        let t = tracks(vec![track("d", pure(ControlMap::sound("bd")))]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..144_000, &mut id);
        let starts: Vec<u64> = evs.iter().map(|v| v.start_frame).collect();
        assert_eq!(starts, vec![0, 48_000, 96_000]);
        assert_eq!(id, 3);
    }

    #[test]
    fn schedule_span_scales_frames_with_cps() {
        // cps = 2 → 24_000 frames/cycle; the two onsets land at 0 and the half-cycle.
        let pat = seq(vec![
            pure(ControlMap::sound("bd")),
            pure(ControlMap::sound("bd")),
        ]);
        let t = tracks(vec![track("d", pat)]);
        let e = Epoch::start(2.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..24_000, &mut id);
        let starts: Vec<u64> = evs.iter().map(|v| v.start_frame).collect();
        assert_eq!(starts, vec![0, 12_000]);
    }

    #[test]
    fn schedule_span_assigns_track_indices() {
        // Two tracks, each one onset at frame 0 → track 0 then track 1 in order.
        let t = tracks(vec![
            track("a", pure(ControlMap::sound("bd"))),
            track("b", pure(ControlMap::sound("sn"))),
        ]);
        let e = Epoch::start(1.0);
        let mut id = 0;
        let evs = schedule_span(&t, &e, 48_000, 0..48_000, &mut id);
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].track, 0);
        assert_eq!(evs[1].track, 1);
    }

    #[test]
    fn schedule_span_alternates_cat_across_cycles() {
        use arbor_grove_pattern::prelude::cat;
        // `< a b >`: "a" on cycle 0, "b" on cycle 1.
        let pat = cat(vec![
            pure(ControlMap::sound("a")),
            pure(ControlMap::sound("b")),
        ]);
        let t = tracks(vec![track("d", pat)]);
        let e = Epoch::start(1.0);
        let sr = 48_000;
        let mut id = 0;
        let evs = schedule_span(&t, &e, sr, 0..96_000, &mut id);
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].start_frame, 0);
        assert_eq!(evs[1].start_frame, 48_000);
        let names: Vec<Option<&str>> = evs
            .iter()
            .map(|v| match &v.source {
                VoiceSource::Named { sound, .. } => sound.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec![Some("a"), Some("b")]);
    }
}
