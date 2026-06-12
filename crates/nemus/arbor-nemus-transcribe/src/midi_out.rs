//! Detected notes → an owned [`Smf`]. Pitched notes and drums go to separate
//! tracks (so the downstream converter, which splits on channel 9, sees clean
//! parts); a Tempo meta rides on the first track. No track-name metas — that
//! would borrow runtime data and the `Smf` must be `'static`; the converter names
//! parts on its own.

use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::note::DetNote;

/// Build an owned MIDI from detected notes at `tempo_bpm` and `ppq` resolution.
pub fn notes_to_smf(notes: &[DetNote], tempo_bpm: f64, ppq: u16) -> Smf<'static> {
    let ppq = ppq.max(1);
    let header = Header::new(Format::Parallel, Timing::Metrical(u15::new(ppq)));
    let mut smf = Smf::new(header);

    // ticks-per-second = ppq * (bpm / 60).
    let tps = ppq as f64 * tempo_bpm.max(1.0) / 60.0;
    let to_tick = |sec: f64| (sec * tps).round().max(0.0) as u64;
    let tempo_us = (60_000_000.0 / tempo_bpm.max(1.0)).round() as u32;

    let pitched: Vec<&DetNote> = notes.iter().filter(|n| n.channel != 9).collect();
    let drums: Vec<&DetNote> = notes.iter().filter(|n| n.channel == 9).collect();

    // The first track always carries the tempo, even if there are no pitched
    // notes (a drums-only transcription still needs a tempo somewhere).
    if pitched.is_empty() && !drums.is_empty() {
        smf.tracks.push(build_track(&drums, Some(tempo_us), &to_tick));
    } else {
        smf.tracks
            .push(build_track(&pitched, Some(tempo_us), &to_tick));
        if !drums.is_empty() {
            smf.tracks.push(build_track(&drums, None, &to_tick));
        }
    }
    smf
}

/// One MIDI event before delta-encoding. `order` breaks ties at the same tick so
/// note-offs sort before note-ons (no spurious zero-length overlaps).
struct AbsEvent {
    tick: u64,
    order: u8,
    kind: TrackEventKind<'static>,
}

fn build_track(
    notes: &[&DetNote],
    tempo_us: Option<u32>,
    to_tick: &impl Fn(f64) -> u64,
) -> Vec<TrackEvent<'static>> {
    let mut evs: Vec<AbsEvent> = Vec::with_capacity(notes.len() * 2 + 2);
    for n in notes {
        let start = to_tick(n.start_sec);
        let end = to_tick(n.start_sec + n.dur_sec).max(start + 1);
        let ch = u4::new(n.channel);
        evs.push(AbsEvent {
            tick: start,
            order: 1,
            kind: TrackEventKind::Midi {
                channel: ch,
                message: MidiMessage::NoteOn {
                    key: u7::new(n.pitch),
                    vel: u7::new(n.vel),
                },
            },
        });
        evs.push(AbsEvent {
            tick: end,
            order: 0,
            kind: TrackEventKind::Midi {
                channel: ch,
                message: MidiMessage::NoteOff {
                    key: u7::new(n.pitch),
                    vel: u7::new(0),
                },
            },
        });
    }
    evs.sort_by(|a, b| a.tick.cmp(&b.tick).then(a.order.cmp(&b.order)));

    let mut out: Vec<TrackEvent<'static>> = Vec::with_capacity(evs.len() + 2);
    let mut prev = 0u64;
    if let Some(us) = tempo_us {
        out.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us))),
        });
    }
    for ev in evs {
        let delta = (ev.tick - prev) as u32;
        prev = ev.tick;
        out.push(TrackEvent {
            delta: u28::new(delta),
            kind: ev.kind,
        });
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

    fn n(start: f64, dur: f64, pitch: u8, channel: u8) -> DetNote {
        DetNote {
            start_sec: start,
            dur_sec: dur,
            pitch,
            vel: 90,
            channel,
        }
    }

    #[test]
    fn splits_pitched_and_drums_into_tracks() {
        let notes = vec![n(0.0, 0.5, 60, 0), n(0.0, 0.1, 36, 9)];
        let smf = notes_to_smf(&notes, 120.0, 480);
        assert_eq!(smf.tracks.len(), 2);
    }

    #[test]
    fn round_trips_through_midly_bytes() {
        // The owned Smf must serialise and re-parse (proves it's well-formed).
        let notes = vec![n(0.0, 0.25, 60, 0), n(0.5, 0.25, 64, 0)];
        let smf = notes_to_smf(&notes, 120.0, 480);
        let mut buf = Vec::new();
        smf.write(&mut buf).expect("write");
        let parsed = Smf::parse(&buf).expect("reparse");
        assert_eq!(parsed.tracks.len(), 1);
    }

    #[test]
    fn drums_only_still_carries_tempo() {
        let notes = vec![n(0.0, 0.05, 36, 9)];
        let smf = notes_to_smf(&notes, 100.0, 480);
        assert_eq!(smf.tracks.len(), 1);
        let has_tempo = smf.tracks[0]
            .iter()
            .any(|e| matches!(e.kind, TrackEventKind::Meta(MetaMessage::Tempo(_))));
        assert!(has_tempo);
    }
}
