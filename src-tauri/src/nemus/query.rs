//! Off-thread arrangement query for the timeline view.
//!
//! `active_haps` (the live editor highlight) only reports what sounds *now*; the
//! arrangement view needs the whole timeline. [`nemus_query`] queries the
//! last-evaluated `Tracks` over `[0, cycles)` — on the command thread, never the
//! audio thread — and returns every hap. The arrangement is cloned under the
//! mutex and the lock dropped before the (potentially heavy) query runs.

use serde::Serialize;
use tauri::State;

use arbor_nemus::prelude::{ControlMap, Time, TimeSpan, Tracks};

use super::events::Diagnostic;
use super::NemusState;
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

/// The `nemus_query` result: every hap + every named section over the window.
#[derive(Debug, Clone, Serialize)]
pub struct QueryHaps {
    pub haps: Vec<QueryHap>,
    /// Named section bands (empty unless a track uses `arrange(section(...))`).
    pub sections: Vec<QuerySection>,
    /// Period (in cycles) after which the whole arrangement repeats — the natural
    /// render length. A `Pattern` has no length, but a song does: `melody(<8…>)` +
    /// `bass(<8…>)` loops every 8 cycles. `0` only when there are no haps at all.
    pub loop_cycles: u32,
    /// The arrangement's effective render tempo (cycles/second): the starting
    /// `tempo(...)` point, else the script's `cps(...)`, mirroring how
    /// `nemus_render` picks the offline-bounce tempo. `None` when the script set
    /// neither (the caller falls back to its configured default). Lets a passive
    /// render estimate stay correct without the transport running.
    pub cps: Option<f64>,
}

/// The `nemus_eval_snippet` result: an arbitrary `.nemus` chunk evaluated in
/// isolation. Mirrors [`QueryHaps`] (the events it generates + its detected loop
/// period + render tempo) but adds inline `diagnostics` — a snippet is editable
/// scratch, so parse/eval errors are surfaced to the panel rather than swallowed.
/// Spans on `haps` are byte offsets relative to the **snippet** text (offset 0 =
/// start of the snippet), so the front end maps them back by adding the snippet's
/// origin offset in the document.
#[derive(Debug, Clone, Serialize)]
pub struct SnippetEval {
    /// Parse/eval/validation errors (empty on success). Inline only — never
    /// emitted on `nemus:diagnostics` (that channel belongs to the main editor).
    pub diagnostics: Vec<Diagnostic>,
    pub haps: Vec<QueryHap>,
    pub sections: Vec<QuerySection>,
    /// Detected loop period (cycles), the natural one-shot length. `0` when empty.
    pub loop_cycles: u32,
    /// Effective render tempo (starting `tempo(...)` point, else `cps(...)`).
    pub cps: Option<f64>,
}

/// Detect the arrangement's loop period (in cycles) over a query window.
///
/// Strategy, most-authoritative first:
/// 1. **Explicit `arrange(...)`** — if any track carries a named-section layout
///    its `period` is the loop length by construction; the longest wins.
/// 2. **Periodicity of the haps** — find the smallest `P` in `1..=window` for
///    which the hap set is invariant under a shift of `P` (the haps in
///    `[0, window-P)` match, modulo the time shift, those in `[P, window)`).
/// 3. **Fallback** — no repetition detected (one-shot / non-periodic content):
///    use the content end (max hap end, rounded up). Never `0` when haps exist.
fn detect_loop_cycles(haps: &[QueryHap], sections: &[QuerySection], window: u32) -> u32 {
    // (1) Explicit arrangement: a named layout's period is the loop, exactly.
    if !sections.is_empty() {
        let period = sections
            .iter()
            .map(|s| s.end.ceil() as u32)
            .max()
            .unwrap_or(0);
        if period > 0 {
            return period.min(window);
        }
    }

    if haps.is_empty() {
        return 0;
    }

    // (2) Periodicity: the smallest shift P that leaves the hap set invariant.
    // A hap's identity for matching is everything but its absolute time; two haps
    // are "the same event one period apart" when they agree on track/duration/
    // payload and their starts differ by exactly P.
    let window_i = window as f64;
    for p in 1..=window {
        let shift = p as f64;
        // Every hap with start < window - P must reappear shifted by +P. Align the
        // two bands onto the same `[0, window - P)` frame: keep the lower band as
        // is, and shift the upper band (`start >= P`) *down* by P. A periodic signal
        // makes the two key-sets identical.
        let mut lower: Vec<HapKey> = haps
            .iter()
            .filter(|h| h.start + EPS < window_i - shift)
            .map(|h| HapKey::shifted(h, 0.0))
            .collect();
        let mut upper: Vec<HapKey> = haps
            .iter()
            .filter(|h| h.start + EPS >= shift)
            .map(|h| HapKey::shifted(h, shift))
            .collect();
        if lower.len() != upper.len() {
            continue;
        }
        lower.sort();
        upper.sort();
        if lower == upper && !lower.is_empty() {
            return p;
        }
    }

    // (3) Fallback: round the content end up to whole cycles.
    let content_end = haps.iter().fold(0.0_f64, |m, h| m.max(h.end));
    (content_end.ceil() as u32).clamp(1, window)
}

/// Tolerance for matching cycle positions across a period shift (haps carry
/// rational times rendered to `f64`, so exact equality is unsafe).
const EPS: f64 = 1e-6;

/// Probe window (in cycles) for evaluating an ad-hoc snippet (no caller-supplied
/// `cycles`): wide enough for [`detect_loop_cycles`] to find periodicity, while
/// still bounding the query of a non-looping snippet to its content end.
pub(super) const SNIPPET_WINDOW: u32 = 64;

/// Query a `Tracks<ControlMap>` over `[0, window)` and collect every hap + every
/// tiled named section, then detect the loop period. The shared core behind
/// [`nemus_query`] (over the staged arrangement) and the snippet evaluator (over
/// a freshly-evaluated `EvalOutput`). Pure — no lock, no `State`.
pub(super) fn collect_haps(
    tracks: &Tracks<ControlMap>,
    window: u32,
) -> (Vec<QueryHap>, Vec<QuerySection>, u32) {
    let window = window.max(1);
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
    let loop_cycles = detect_loop_cycles(&haps, &sections, window);
    (haps, sections, loop_cycles)
}

/// A hap's identity for periodicity matching: track + duration + payload + onset,
/// with the *start* quantised after the candidate shift so two events a period
/// apart compare equal. Times are quantised to a fine grid to dodge `f64` noise.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct HapKey {
    track: u32,
    start_q: i64,
    dur_q: i64,
    has_onset: bool,
    sound: Option<String>,
    note_q: Option<i64>,
}

impl HapKey {
    fn shifted(h: &QueryHap, shift: f64) -> Self {
        const GRID: f64 = 1_000_000.0;
        let q = |x: f64| (x * GRID).round() as i64;
        HapKey {
            track: h.track,
            start_q: q(h.start - shift),
            dur_q: q(h.end - h.start),
            has_onset: h.has_onset,
            sound: h.sound.clone(),
            note_q: h.note.map(q),
        }
    }
}

/// Query the last-evaluated arrangement over `[0, cycles)` and return every hap.
/// Empty when nothing has been evaluated yet. Off the audio thread, so it's safe
/// to call while playing.
#[tauri::command]
pub async fn nemus_query(
    nemus: State<'_, NemusState>,
    cycles: u32,
) -> Result<QueryHaps, AppError> {
    // Clone the arrangement under the lock, then drop it before querying. Capture
    // the render tempo too (starting `tempo(...)` point, else `cps(...)`) — the
    // same choice `nemus_render` makes — so the estimate is right without playback.
    let (tracks, cps): (Option<Tracks<ControlMap>>, Option<f64>) = {
        let latest = nemus.latest.lock().unwrap_or_else(|e| e.into_inner());
        match latest.as_ref() {
            Some(l) => (Some(l.tracks.clone()), l.tempo.points.first().map(|p| p.1).or(l.cps)),
            None => (None, None),
        }
    };

    let Some(tracks) = tracks else {
        return Ok(QueryHaps { haps: Vec::new(), sections: Vec::new(), loop_cycles: 0, cps: None });
    };

    let (haps, sections, loop_cycles) = collect_haps(&tracks, cycles);
    Ok(QueryHaps { haps, sections, loop_cycles, cps })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hap(track: u32, start: f64, end: f64, sound: &str) -> QueryHap {
        QueryHap {
            track,
            start,
            end,
            has_onset: true,
            span_start: None,
            span_end: None,
            sound: Some(sound.to_string()),
            note: None,
            gain: None,
        }
    }

    /// A song that loops every 8 cycles, queried over a 96-cycle window: track 0
    /// fires `bd` on every cycle, track 1 fires `bass` once at the head of each
    /// 8-cycle block. The detector must find period 8, not the window 96.
    #[test]
    fn detects_period_8_over_window_96() {
        let window = 96;
        let mut haps = Vec::new();
        for c in 0..window {
            haps.push(hap(0, f64::from(c), f64::from(c) + 1.0, "bd"));
            if c % 8 == 0 {
                haps.push(hap(1, f64::from(c), f64::from(c) + 1.0, "bass"));
            }
        }
        assert_eq!(detect_loop_cycles(&haps, &[], window), 8);
    }

    /// An explicit `arrange(...)` reports its total via the named-section layout;
    /// the period comes straight from the sections (max `end`), not the haps.
    #[test]
    fn explicit_arrange_uses_section_total() {
        let sections = vec![
            QuerySection { track: 0, name: "INTRO".into(), start: 0.0, end: 4.0 },
            QuerySection { track: 0, name: "VERSE".into(), start: 4.0, end: 12.0 },
        ];
        // Haps are incidental here; the section total (12) is authoritative.
        let haps = vec![hap(0, 0.0, 1.0, "bd"), hap(0, 4.0, 1.0, "sn")];
        assert_eq!(detect_loop_cycles(&haps, &sections, 96), 12);
    }

    /// Non-periodic one-shot content falls back to the rounded-up content end,
    /// and never returns 0 while haps exist.
    #[test]
    fn fallback_to_content_end_when_not_periodic() {
        let haps = vec![hap(0, 0.0, 2.5, "bd"), hap(0, 5.0, 6.0, "sn")];
        assert_eq!(detect_loop_cycles(&haps, &[], 96), 6);
    }
}
