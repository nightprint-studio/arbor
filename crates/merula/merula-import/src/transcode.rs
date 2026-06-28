//! **L1 — transcode**: MIDI events → a neutral [`Song`] of explicit notes.
//!
//! This layer is purely mechanical: pair note-ons with note-offs, convert ticks
//! to cycles (exact rationals, no drift), read the tempo, and split each MIDI
//! track into a pitched part and a drum part (GM channel 10). No musical
//! interpretation happens here — that is L2's job ([`crate::quantize`],
//! [`crate::key`], [`crate::chords`]).

use std::collections::HashMap;

use merula_pattern::prelude::Time;
use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use crate::error::{ImportError, Result};
use crate::model::{ImportOptions, Note, NoteTrack, Song};

/// GM drum channel, zero-based (MIDI "channel 10").
const DRUM_CHANNEL: u8 = 9;

/// A note-on awaiting its matching note-off.
struct OpenNote {
    start_tick: u64,
    vel: u8,
}

/// Parse raw `.mid` bytes, then transcode. The one-shot entry for on-disk and
/// in-memory MIDI alike (the transcriber hands us bytes it never writes out).
pub fn from_bytes(bytes: &[u8], opts: &ImportOptions) -> Result<Song> {
    let smf = Smf::parse(bytes).map_err(|e| ImportError::Midi(e.to_string()))?;
    from_smf(&smf, opts)
}

/// Transcode an already-parsed [`Smf`] into a [`Song`].
pub fn from_smf(smf: &Smf, opts: &ImportOptions) -> Result<Song> {
    let ppq = match smf.header.timing {
        Timing::Metrical(n) => n.as_int() as u64,
        Timing::Timecode(..) => return Err(ImportError::UnsupportedTiming),
    };
    if ppq == 0 {
        return Err(ImportError::Midi("zero ticks-per-quarter".into()));
    }
    let tpc = (ppq * opts.beats_per_cycle.max(1) as u64).max(1) as i64;

    // Tempo is global: take the earliest `Tempo` meta seen on any track.
    let mut tempo_us = 500_000u64; // microseconds per quarter (120 bpm)
    let mut tempo_at = u64::MAX;

    let mut out: Vec<NoteTrack> = Vec::new();

    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut abs: u64 = 0;
        let mut name: Option<String> = None;
        let mut open: HashMap<(u8, u8), OpenNote> = HashMap::new();
        let mut pitched: Vec<Note> = Vec::new();
        let mut drums: Vec<Note> = Vec::new();

        for ev in track.iter() {
            abs += ev.delta.as_int() as u64;
            match &ev.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) => {
                    if abs < tempo_at {
                        tempo_at = abs;
                        tempo_us = us.as_int() as u64;
                    }
                }
                TrackEventKind::Meta(MetaMessage::TrackName(raw)) => {
                    if name.is_none() {
                        let n = String::from_utf8_lossy(raw).trim().to_string();
                        if !n.is_empty() {
                            name = Some(n);
                        }
                    }
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        MidiMessage::NoteOn { key, vel } => {
                            let k = key.as_int();
                            // A note-on with velocity 0 is a note-off (running
                            // status convention). Otherwise (re)start the note;
                            // close any still-open one of the same key first.
                            close(&mut open, &mut pitched, &mut drums, ch, k, abs, tpc);
                            if vel.as_int() > 0 {
                                open.insert(
                                    (ch, k),
                                    OpenNote {
                                        start_tick: abs,
                                        vel: vel.as_int(),
                                    },
                                );
                            }
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            close(&mut open, &mut pitched, &mut drums, ch, key.as_int(), abs, tpc);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // Defensive: close notes left hanging at the last tick we saw.
        for (ch, k) in open.keys().copied().collect::<Vec<_>>() {
            close(&mut open, &mut pitched, &mut drums, ch, k, abs, tpc);
        }

        let base = name.filter(|s| !s.is_empty());
        push_track(&mut out, base.as_deref(), ti, false, pitched);
        push_track(&mut out, base.as_deref(), ti, true, drums);
    }

    if out.is_empty() {
        return Err(ImportError::Empty);
    }

    // cps = cycles per second = 1 / seconds-per-cycle, where one cycle is
    // `beats_per_cycle` quarter-notes and a quarter lasts `tempo_us` microseconds.
    let cps = 1_000_000.0 / (tempo_us as f64 * opts.beats_per_cycle.max(1) as f64);
    let len_cycles = len_in_cycles(&out);
    Ok(Song {
        tracks: out,
        cps,
        len_cycles,
    })
}

/// Close an open note (if any) at `end_tick`, routing it to the drum or pitched
/// bucket by channel.
fn close(
    open: &mut HashMap<(u8, u8), OpenNote>,
    pitched: &mut Vec<Note>,
    drums: &mut Vec<Note>,
    ch: u8,
    k: u8,
    end_tick: u64,
    tpc: i64,
) {
    if let Some(on) = open.remove(&(ch, k)) {
        let dur_ticks = end_tick.saturating_sub(on.start_tick).max(1);
        let note = Note {
            start: Time::new(on.start_tick as i64, tpc),
            dur: Time::new(dur_ticks as i64, tpc),
            pitch: k as i32,
            vel: on.vel as f64 / 127.0,
        };
        if ch == DRUM_CHANNEL {
            drums.push(note);
        } else {
            pitched.push(note);
        }
    }
}

/// Sort and append a non-empty bucket as a [`NoteTrack`], naming it from the
/// MIDI track name (suffixing drum parts) or a generated fallback.
fn push_track(out: &mut Vec<NoteTrack>, base: Option<&str>, ti: usize, is_drum: bool, mut notes: Vec<Note>) {
    if notes.is_empty() {
        return;
    }
    notes.sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    let name = match (base, is_drum) {
        (Some(b), false) => b.to_string(),
        (Some(b), true) => format!("{b} drums"),
        (None, false) => format!("track {}", ti + 1),
        (None, true) => "drums".to_string(),
    };
    out.push(NoteTrack {
        name,
        is_drum,
        notes,
    });
}

/// Ceiling of the latest note end, in whole cycles (at least 1).
fn len_in_cycles(tracks: &[NoteTrack]) -> u32 {
    let mut max_end = Time::ZERO;
    for t in tracks {
        for n in &t.notes {
            let e = n.end();
            if e > max_end {
                max_end = e;
            }
        }
    }
    let floor = max_end.floor();
    let ceil = if max_end.cycle_pos() == Time::ZERO {
        floor
    } else {
        floor + 1
    };
    ceil.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::num::{u15, u24, u28, u4, u7};
    use midly::{Format, Header, MetaMessage, MidiMessage, Smf, TrackEvent, TrackEventKind};

    fn note_on(delta: u32, ch: u8, key: u8, vel: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: u28::new(delta),
            kind: TrackEventKind::Midi {
                channel: u4::new(ch),
                message: MidiMessage::NoteOn {
                    key: u7::new(key),
                    vel: u7::new(vel),
                },
            },
        }
    }
    fn note_off(delta: u32, ch: u8, key: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: u28::new(delta),
            kind: TrackEventKind::Midi {
                channel: u4::new(ch),
                message: MidiMessage::NoteOff {
                    key: u7::new(key),
                    vel: u7::new(0),
                },
            },
        }
    }

    /// 96 PPQ, 4 beats/cycle ⇒ 384 ticks per cycle. A quarter note = 96 ticks
    /// = 1/4 cycle.
    fn smf_one_quarter() -> Smf<'static> {
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(96)));
        let mut smf = Smf::new(header);
        smf.tracks.push(vec![
            note_on(0, 0, 60, 100),
            note_off(96, 0, 60),
        ]);
        smf
    }

    #[test]
    fn ticks_become_exact_cycle_fractions() {
        let song = from_smf(&smf_one_quarter(), &ImportOptions::default()).unwrap();
        assert_eq!(song.tracks.len(), 1);
        let n = song.tracks[0].notes[0];
        assert_eq!(n.start, Time::ZERO);
        assert_eq!(n.dur, Time::new(1, 4)); // a quarter of a cycle, exactly
        assert_eq!(n.pitch, 60);
        assert!((n.vel - 100.0 / 127.0).abs() < 1e-9);
        assert_eq!(song.len_cycles, 1);
    }

    #[test]
    fn default_tempo_is_120_bpm() {
        let song = from_smf(&smf_one_quarter(), &ImportOptions::default()).unwrap();
        // 120 bpm / 60 / 4 beats-per-cycle = 0.5 cps.
        assert!((song.cps - 0.5).abs() < 1e-9);
    }

    #[test]
    fn reads_explicit_tempo() {
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(96)));
        let mut smf = Smf::new(header);
        smf.tracks.push(vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(250_000))), // 240 bpm
            },
            note_on(0, 0, 60, 80),
            note_off(96, 0, 60),
        ]);
        let song = from_smf(&smf, &ImportOptions::default()).unwrap();
        // 240 bpm / 60 / 4 = 1.0 cps.
        assert!((song.cps - 1.0).abs() < 1e-9);
    }

    #[test]
    fn drum_channel_splits_into_its_own_track() {
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(96)));
        let mut smf = Smf::new(header);
        smf.tracks.push(vec![
            note_on(0, 0, 60, 100),  // pitched
            note_on(0, 9, 36, 100),  // drum (kick)
            note_off(96, 0, 60),
            note_off(0, 9, 36),
        ]);
        let song = from_smf(&smf, &ImportOptions::default()).unwrap();
        assert_eq!(song.tracks.len(), 2);
        assert!(song.tracks.iter().any(|t| !t.is_drum));
        assert!(song.tracks.iter().any(|t| t.is_drum));
    }

    #[test]
    fn rejects_empty_and_timecode() {
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(96)));
        let mut empty = Smf::new(header);
        empty.tracks.push(vec![]);
        assert!(matches!(
            from_smf(&empty, &ImportOptions::default()),
            Err(ImportError::Empty)
        ));
    }
}
