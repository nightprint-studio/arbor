//! Offline **MIDI export**: walk `Tracks` over a cycle window and write a
//! Standard MIDI File (one MIDI track per merula track). Pure note data — no
//! audio, no `Renderer`, no real time. The dual of the WAV/Ogg bounce in
//! [`render`](crate::render): instead of sample frames it emits note-on/off
//! events the user can drop into any DAW.
//!
//! Mapping:
//! - a hap with a **pitch** (`note`) becomes a channel-0 note;
//! - a hap whose **sound** is a recognised drum name maps to General-MIDI
//!   percussion on channel 9 (so a `bd sn hh` pattern exports as real drums);
//! - everything else (untuned one-shots, continuous signals) is skipped.
//!
//! Tempo: one cycle = one bar of 4/4 = four quarter notes, so `bpm = cps × 240`.
//! Tick positions are tempo-independent (`PPQ × 4` per cycle); the tempo only
//! sets playback speed via a Tempo meta on the first track.

use std::path::Path;

use merula_pattern::prelude::{ControlMap, Time, TimeSpan, Tracks};
use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::error::{EngineError, Result};

/// Pulses-per-quarter-note of the exported file. 480 is the DAW-standard PPQ.
const PPQ: u16 = 480;
/// Ticks per cycle: one cycle = one bar of 4/4 = four quarter notes.
const TICKS_PER_CYCLE: f64 = PPQ as f64 * 4.0;

/// Summary of a completed MIDI export, surfaced to the caller (and the UI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MidiExportSummary {
    /// MIDI tracks written (one per merula track that produced ≥1 note).
    pub tracks: u32,
    /// Total notes written across all tracks.
    pub notes: u32,
}

/// Export `cycles` cycles of `tracks` (at `cps`) to a Standard MIDI File at
/// `out_path`. One MIDI track per merula track that yields any note; the first
/// carries the Tempo meta. An arrangement with no exportable notes still writes
/// a valid file (a single tempo-only track), never a malformed one.
///
/// The render length is an explicit cycle count, like [`render_offline`](crate::render::render_offline):
/// a `Pattern` has no intrinsic length, so the caller decides how many cycles to
/// bake (typically the arrangement's detected loop period).
pub fn export_midi(
    tracks: &Tracks<ControlMap>,
    cps: f64,
    cycles: u32,
    out_path: &Path,
) -> Result<MidiExportSummary> {
    let cycles = cycles.max(1);
    let span = TimeSpan::new(Time::int(0), Time::int(cycles as i64));

    // Tempo: one cycle = one 4/4 bar = four beats, so bpm = cps × 240.
    let bpm = (cps * 240.0).clamp(1.0, 1_000.0);
    let tempo_us = (60_000_000.0 / bpm).round() as u32;

    let header = Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ)));
    let mut smf = Smf::new(header);

    let mut total_notes = 0u32;
    let mut written_tracks = 0u32;
    for track in &tracks.tracks {
        let mut notes: Vec<MidiNote> = Vec::new();
        for hap in track.pattern.query(span) {
            // Onsets only — a continuous signal (no `whole`) isn't a note.
            let Some(whole) = hap.whole else { continue };
            let start = whole.begin.to_f64();
            let end = whole.end.to_f64();
            if end <= start {
                continue;
            }
            let Some((key, channel)) = note_for(&hap.value) else { continue };
            notes.push(MidiNote {
                start_tick: (start * TICKS_PER_CYCLE).round().max(0.0) as u64,
                end_tick: (end * TICKS_PER_CYCLE).round().max(0.0) as u64,
                key,
                channel,
                vel: velocity_for(&hap.value),
            });
        }
        if notes.is_empty() {
            continue;
        }
        total_notes += notes.len() as u32;
        // Tempo rides on the first written track; every track is named.
        let tempo = (written_tracks == 0).then_some(tempo_us);
        smf.tracks.push(build_track(&notes, &track.name, tempo));
        written_tracks += 1;
    }

    // No exportable notes anywhere → still emit a valid file: one track carrying
    // just the tempo (an empty-but-well-formed SMF, not a malformed one).
    if smf.tracks.is_empty() {
        smf.tracks.push(build_track(&[], "", Some(tempo_us)));
        written_tracks = 1;
    }

    let mut buf = Vec::new();
    smf.write(&mut buf)
        .map_err(|e| EngineError::Render(format!("writing MIDI: {e}")))?;
    std::fs::write(out_path, &buf)
        .map_err(|e| EngineError::Io(format!("creating {}: {e}", out_path.display())))?;

    Ok(MidiExportSummary { tracks: written_tracks, notes: total_notes })
}

/// One note collected from a hap, in absolute ticks (pre delta-encoding).
struct MidiNote {
    start_tick: u64,
    end_tick: u64,
    key: u8,
    channel: u4,
    vel: u7,
}

/// The MIDI key + channel a hap maps to, or `None` if it isn't a note (an
/// untuned one-shot with no recognised drum name).
fn note_for(cm: &ControlMap) -> Option<(u8, u4)> {
    if let Some(n) = cm.note {
        let key = n.round().clamp(0.0, 127.0) as u8;
        return Some((key, u4::new(0)));
    }
    // Drum sound → General-MIDI percussion on channel 9.
    cm.sound
        .as_deref()
        .and_then(gm_drum)
        .map(|key| (key, u4::new(9)))
}

/// Velocity from per-hap gain (`0..1` → `1..127`), defaulting to a musical mezzo
/// when the hap carries no gain. Never 0 (a zero-velocity note-on is a note-off).
fn velocity_for(cm: &ControlMap) -> u7 {
    match cm.gain {
        Some(g) => u7::new((g.clamp(0.0, 1.0) * 127.0).round().clamp(1.0, 127.0) as u8),
        None => u7::new(100),
    }
}

/// Map a merula drum sound name to a General-MIDI percussion key, or `None` for a
/// pitched/untuned sound. A trailing sample index (`bd:3`) is stripped first.
fn gm_drum(name: &str) -> Option<u8> {
    let base = name.split(':').next().unwrap_or(name);
    Some(match base {
        "bd" | "kick" => 36,        // Bass Drum 1
        "sd" | "sn" | "snare" => 38, // Acoustic Snare
        "rim" | "rs" => 37,         // Side Stick
        "cp" | "clap" => 39,        // Hand Clap
        "hh" | "ch" | "hat" => 42,  // Closed Hi-Hat
        "oh" | "open" => 46,        // Open Hi-Hat
        "lt" | "lowtom" => 45,      // Low Tom
        "mt" | "midtom" => 47,      // Low-Mid Tom
        "ht" | "hitom" => 50,       // High Tom
        "cr" | "crash" => 49,       // Crash Cymbal 1
        "rd" | "ride" => 51,        // Ride Cymbal 1
        "cb" | "cowbell" => 56,     // Cowbell
        _ => return None,
    })
}

/// One MIDI event before delta-encoding. `order` breaks ties at the same tick so
/// note-offs sort before note-ons (no spurious zero-length overlaps).
struct AbsEvent {
    tick: u64,
    order: u8,
    kind: TrackEventKind<'static>,
}

/// Build one MIDI track from `notes`, prefixed by an optional Tempo meta and a
/// TrackName meta (when `name` is non-empty). Borrows `name` for the returned
/// events, so the SMF must be serialised while `name` is alive.
fn build_track<'a>(notes: &[MidiNote], name: &'a str, tempo_us: Option<u32>) -> Vec<TrackEvent<'a>> {
    let mut evs: Vec<AbsEvent> = Vec::with_capacity(notes.len() * 2);
    for n in notes {
        let start = n.start_tick;
        let end = n.end_tick.max(start + 1);
        evs.push(AbsEvent {
            tick: start,
            order: 1,
            kind: TrackEventKind::Midi {
                channel: n.channel,
                message: MidiMessage::NoteOn { key: u7::new(n.key), vel: n.vel },
            },
        });
        evs.push(AbsEvent {
            tick: end,
            order: 0,
            kind: TrackEventKind::Midi {
                channel: n.channel,
                message: MidiMessage::NoteOff { key: u7::new(n.key), vel: u7::new(0) },
            },
        });
    }
    evs.sort_by(|a, b| a.tick.cmp(&b.tick).then(a.order.cmp(&b.order)));

    let mut out: Vec<TrackEvent<'a>> = Vec::with_capacity(evs.len() + 3);
    if let Some(us) = tempo_us {
        out.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us))),
        });
    }
    if !name.is_empty() {
        out.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(name.as_bytes())),
        });
    }
    let mut prev = 0u64;
    for ev in evs {
        let delta = (ev.tick - prev) as u32;
        prev = ev.tick;
        out.push(TrackEvent { delta: u28::new(delta), kind: ev.kind });
    }
    out.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use merula_pattern::prelude::{fastcat, pure, sample, track, tracks};

    /// A two-note pitched track exports to one MIDI track that re-parses cleanly.
    #[test]
    fn exports_pitched_notes_to_a_reparseable_file() {
        // `c4 e4` as a two-step cycle, built from the pure algebra (the engine
        // doesn't depend on the lang front-end / mini-notation).
        let melody = fastcat(vec![pure(ControlMap::note(60.0)), pure(ControlMap::note(64.0))]);
        let t = tracks(vec![track("lead", melody)]);
        let out = std::env::temp_dir().join("merula_export_pitched.mid");
        let summary = export_midi(&t, 1.0, 1, &out).expect("export");
        assert_eq!(summary.tracks, 1);
        assert_eq!(summary.notes, 2);
        let bytes = std::fs::read(&out).expect("read back");
        let smf = Smf::parse(&bytes).expect("reparse");
        assert_eq!(smf.tracks.len(), 1);
        let _ = std::fs::remove_file(&out);
    }

    /// Recognised drum names map to channel-9 percussion.
    #[test]
    fn drum_names_map_to_gm_percussion() {
        assert_eq!(gm_drum("bd"), Some(36));
        assert_eq!(gm_drum("hh:3"), Some(42)); // sample index stripped
        assert_eq!(gm_drum("supersaw"), None);
    }

    /// An empty / unexportable arrangement still writes a valid tempo-only file.
    #[test]
    fn empty_arrangement_writes_valid_file() {
        let t = tracks(vec![track("noise", sample("does-not-map"))]);
        let out = std::env::temp_dir().join("merula_export_empty.mid");
        let summary = export_midi(&t, 0.5, 1, &out).expect("export");
        assert_eq!(summary.notes, 0);
        assert_eq!(summary.tracks, 1);
        let bytes = std::fs::read(&out).expect("read back");
        assert!(Smf::parse(&bytes).is_ok());
        let _ = std::fs::remove_file(&out);
    }
}
