//! End-to-end: build a small MIDI, run the full deterministic pipeline, and
//! assert the emitted `.nemus` re-parses and carries the expected idioms.

use arbor_nemus_import::prelude::{smf_to_nemus, ImportOptions};
use arbor_nemus_lang::prelude::parse;
use midly::num::{u15, u28, u4, u7};
use midly::{Format, Header, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

fn on(delta: u32, ch: u8, key: u8) -> TrackEvent<'static> {
    TrackEvent {
        delta: u28::new(delta),
        kind: TrackEventKind::Midi {
            channel: u4::new(ch),
            message: MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(96),
            },
        },
    }
}
fn off(delta: u32, ch: u8, key: u8) -> TrackEvent<'static> {
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

/// 96 PPQ; a held C-major triad (one bar) on a pitched track, plus a kick on
/// the drum channel — exercises chord folding and drum mapping at once.
fn demo() -> Smf<'static> {
    let header = Header::new(Format::Parallel, Timing::Metrical(u15::new(96)));
    let mut smf = Smf::new(header);
    // Pitched: C E G struck together, sustained a whole bar (384 ticks).
    smf.tracks.push(vec![
        on(0, 0, 60),
        on(0, 0, 64),
        on(0, 0, 67),
        off(384, 0, 60),
        off(0, 0, 64),
        off(0, 0, 67),
    ]);
    // Drums: a kick on beat 1.
    smf.tracks.push(vec![on(0, 9, 36), off(96, 9, 36)]);
    smf
}

#[test]
fn full_pipeline_emits_parseable_idiomatic_nemus() {
    let src = smf_to_nemus(&demo(), &ImportOptions::default()).unwrap();
    assert!(parse(&src).is_ok(), "must re-parse:\n{src}");
    // Two tracks → a tracks(...) output with named channels.
    assert!(src.contains("tracks("), "expected multi-track output:\n{src}");
    assert!(src.contains("'maj"), "expected a chord symbol:\n{src}");
    assert!(src.contains("bd"), "expected the GM kick mapped to bd:\n{src}");
    assert!(src.contains("cps("), "expected a tempo statement:\n{src}");
}
