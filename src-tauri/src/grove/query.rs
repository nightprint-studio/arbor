//! Off-thread arrangement query for the timeline view.
//!
//! `active_haps` (the live editor highlight) only reports what sounds *now*; the
//! arrangement view needs the whole timeline. [`grove_query`] queries the
//! last-evaluated `Tracks` over `[0, cycles)` — on the command thread, never the
//! audio thread — and returns every hap. The arrangement is cloned under the
//! mutex and the lock dropped before the (potentially heavy) query runs.

use serde::Serialize;
use tauri::State;

use arbor_grove::prelude::{ControlMap, Time, TimeSpan, Tracks};

use super::GroveState;
use crate::error::AppError;

/// One hap of the queried arrangement. `start`/`end` are in cycles (absolute
/// timeline); `has_onset` is false for continuous signals (no `whole`).
#[derive(Debug, Clone, Serialize)]
pub struct QueryHap {
    /// Mixer-strip / arrangement-lane index (0-based).
    pub track: u32,
    /// Onset in cycles (the hap's `whole.begin`, or `part.begin` if continuous).
    pub start: f64,
    /// End in cycles (`whole.end`, or `part.end` if continuous).
    pub end: f64,
    /// True for a discrete event (has a `whole`); false for a continuous signal.
    pub has_onset: bool,
    /// Source byte-range start (for editor mapping), or `None`.
    pub span_start: Option<u32>,
    /// Source byte-range end, or `None`.
    pub span_end: Option<u32>,
    /// Sound name (`bd`, …) if any.
    pub sound: Option<String>,
    /// MIDI pitch if any.
    pub note: Option<f64>,
    /// Per-hap gain if any.
    pub gain: Option<f64>,
}

/// One named arrangement section, tiled to an absolute cycle range within the
/// queried window (the arrangement loops, so a section repeats every period).
#[derive(Debug, Clone, Serialize)]
pub struct QuerySection {
    /// Owning mixer-strip / arrangement-lane index (0-based).
    pub track: u32,
    /// Section label (`section("INTRO", …)`).
    pub name: String,
    /// Start cycle (absolute, inclusive).
    pub start: f64,
    /// End cycle (absolute, exclusive).
    pub end: f64,
}

/// The `grove_query` result: every hap + every named section over the window.
#[derive(Debug, Clone, Serialize)]
pub struct QueryHaps {
    pub haps: Vec<QueryHap>,
    /// Named section bands (empty unless a track uses `arrange(section(...))`).
    pub sections: Vec<QuerySection>,
}

/// Query the last-evaluated arrangement over `[0, cycles)` and return every hap.
/// Empty when nothing has been evaluated yet. Off the audio thread, so it's safe
/// to call while playing.
#[tauri::command]
pub async fn grove_query(
    grove: State<'_, GroveState>,
    cycles: u32,
) -> Result<QueryHaps, AppError> {
    // Clone the arrangement under the lock, then drop it before querying.
    let tracks: Option<Tracks<ControlMap>> = {
        let latest = grove.latest.lock().unwrap_or_else(|e| e.into_inner());
        latest.as_ref().map(|l| l.tracks.clone())
    };

    let Some(tracks) = tracks else {
        return Ok(QueryHaps { haps: Vec::new(), sections: Vec::new() });
    };

    let window = cycles.max(1);
    let span = TimeSpan::new(Time::int(0), Time::int(window as i64));
    let mut haps: Vec<QueryHap> = Vec::new();
    let mut sections: Vec<QuerySection> = Vec::new();
    for (track_idx, track) in tracks.tracks.iter().enumerate() {
        let track_id = track_idx as u32;
        // Tile the named-section layout across the window (the arrangement loops
        // every `period` cycles, so each named span repeats).
        if track.period > 0 && !track.sections.is_empty() {
            let mut base = 0u32;
            while base < window {
                for s in &track.sections {
                    let start = base + s.start;
                    if start >= window {
                        continue;
                    }
                    sections.push(QuerySection {
                        track: track_id,
                        name: s.name.clone(),
                        start: f64::from(start),
                        end: f64::from((base + s.end).min(window)),
                    });
                }
                base += track.period;
            }
        }
        for hap in track.pattern.query(span) {
            // Discrete events carry a `whole`; continuous signals only a `part`.
            let (start, end) = match hap.whole {
                Some(w) => (w.begin.to_f64(), w.end.to_f64()),
                None => (hap.part.begin.to_f64(), hap.part.end.to_f64()),
            };
            haps.push(QueryHap {
                track: track_id,
                start,
                end,
                has_onset: hap.whole.is_some(),
                span_start: hap.span.map(|s| s.start),
                span_end: hap.span.map(|s| s.end),
                sound: hap.value.sound.clone(),
                note: hap.value.note,
                gain: hap.value.gain,
            });
        }
    }
    Ok(QueryHaps { haps, sections })
}
