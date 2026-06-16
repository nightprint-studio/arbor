//! **L2 — quantise**: snap onsets and durations to a grid.
//!
//! Transcription timing is rarely exactly on the grid (human MIDI, or a
//! transcriber's frame-quantised onsets). Snapping to `1/grid` of a cycle makes
//! the emitted mini-notation land on clean slot boundaries, which is both more
//! readable and what makes loop detection (identical cycles) actually fire.

use arbor_nemus_pattern::prelude::Time;

use crate::model::{Note, Song};

/// Snap every note in `song` to a `1/grid` grid (no-op when `grid == 0`).
pub fn quantize_song(song: &mut Song, grid: u32) {
    if grid == 0 {
        return;
    }
    let g = grid as i64;
    for track in &mut song.tracks {
        for n in &mut track.notes {
            quantize_note(n, g);
        }
        dedup(&mut track.notes);
    }
}

/// Snap one note: onset to the nearest cell, duration to the nearest cell but at
/// least one cell long (a hit never collapses to zero length).
fn quantize_note(n: &mut Note, grid: i64) {
    n.start = snap(n.start, grid);
    let d = snap(n.dur, grid);
    n.dur = if d <= Time::ZERO { Time::new(1, grid) } else { d };
}

/// Round a time to the nearest `1/grid` boundary.
fn snap(t: Time, grid: i64) -> Time {
    // k = round(t * grid); result = k / grid.
    let scaled = t * Time::int(grid);
    let k = (scaled + Time::new(1, 2)).floor(); // round half up (times are ≥ 0)
    Time::new(k, grid)
}

/// Collapse notes that snapped onto the same `(start, pitch)`, keeping the
/// longest duration and loudest velocity — overlapping unisons become one note.
fn dedup(notes: &mut Vec<Note>) {
    notes.sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    let mut out: Vec<Note> = Vec::with_capacity(notes.len());
    for n in notes.drain(..) {
        if let Some(last) = out.last_mut() {
            if last.start == n.start && last.pitch == n.pitch {
                last.dur = last.dur.max(n.dur);
                last.vel = last.vel.max(n.vel);
                continue;
            }
        }
        out.push(n);
    }
    *notes = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NoteTrack;

    fn note(start: Time, dur: Time, pitch: i32) -> Note {
        Note {
            start,
            dur,
            pitch,
            vel: 0.8,
        }
    }

    #[test]
    fn snaps_off_grid_onset_to_nearest_cell() {
        // A triplet onset 1/3 with a 1/4 grid → 1/3*4 = 4/3 ≈ 1.33 → 1 → 1/4.
        let mut song = Song {
            tracks: vec![NoteTrack {
                name: "t".into(),
                is_drum: false,
                notes: vec![note(Time::new(1, 3), Time::new(1, 3), 60)],
            }],
            cps: 0.5,
            len_cycles: 1,
        };
        quantize_song(&mut song, 4);
        assert_eq!(song.tracks[0].notes[0].start, Time::new(1, 4));
    }

    #[test]
    fn duration_never_collapses_below_one_cell() {
        let mut song = Song {
            tracks: vec![NoteTrack {
                name: "t".into(),
                is_drum: false,
                notes: vec![note(Time::ZERO, Time::new(1, 100), 60)],
            }],
            cps: 0.5,
            len_cycles: 1,
        };
        quantize_song(&mut song, 16);
        assert_eq!(song.tracks[0].notes[0].dur, Time::new(1, 16));
    }

    #[test]
    fn merges_unisons_that_snap_together() {
        let mut song = Song {
            tracks: vec![NoteTrack {
                name: "t".into(),
                is_drum: false,
                notes: vec![
                    note(Time::new(1, 17), Time::new(1, 4), 60),
                    note(Time::new(1, 15), Time::new(1, 2), 60),
                ],
            }],
            cps: 0.5,
            len_cycles: 1,
        };
        quantize_song(&mut song, 16);
        assert_eq!(song.tracks[0].notes.len(), 1);
        assert_eq!(song.tracks[0].notes[0].dur, Time::new(1, 2)); // longest kept
    }
}
