// TODO(prune): merula moved to merula-be — these commands are no longer registered.
//! The scale catalogue, exposed to the frontend.
//!
//! `merula_scales` returns every mode `.scale("root:mode")` accepts (canonical
//! name + aliases + one-octave semitone intervals) from the authoritative table in
//! `merula-pattern` ([`mode_table`]). The editor loads it once and drives its
//! scale-aware quick-fixes off it — "snap a note to the nearest scale degree" and
//! "change the scale and rewrite the degrees coherently" — so the editor's music
//! theory and the evaluator's can never drift. Static + cheap (borrowed-static
//! data); no state, no I/O. Same boundary pattern as `reference.rs`.

use merula::prelude::mode_table;
use serde::Serialize;

use crate::error::AppError;

/// IPC view of a `ScaleMode`. The JSON field names are the contract the frontend
/// `scalesStore` parses.
#[derive(Serialize)]
pub struct ScaleModeDto {
    name: &'static str,
    aliases: Vec<&'static str>,
    /// Ascending semitone offsets from the root, one octave (e.g. major =
    /// `[0,2,4,5,7,9,11]`).
    intervals: Vec<i32>,
}

/// Return the full scale-mode catalogue.
#[tauri::command]
pub fn merula_scales() -> Result<Vec<ScaleModeDto>, AppError> {
    Ok(mode_table()
        .iter()
        .map(|m| ScaleModeDto {
            name: m.name,
            aliases: m.aliases.to_vec(),
            intervals: m.intervals.to_vec(),
        })
        .collect())
}
