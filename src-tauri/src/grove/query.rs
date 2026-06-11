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

/// The `grove_query` result: every hap over the requested cycle window.
#[derive(Debug, Clone, Serialize)]
pub struct QueryHaps {
    pub haps: Vec<QueryHap>,
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
        return Ok(QueryHaps { haps: Vec::new() });
    };

    let span = TimeSpan::new(Time::int(0), Time::int(cycles.max(1) as i64));
    let mut haps: Vec<QueryHap> = Vec::new();
    for (track_idx, track) in tracks.tracks.iter().enumerate() {
        let track_id = track_idx as u32;
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
    Ok(QueryHaps { haps })
}
