//! `scales` domain — the scale catalogue, exposed to the frontend.
//!
//! `merula_scales` returns every mode `.scale("root:mode")` accepts (canonical
//! name + aliases + one-octave semitone intervals) from the authoritative table in
//! `merula-pattern` ([`mode_table`]). The editor loads it once and drives its
//! scale-aware quick-fixes off it — "snap a note to the nearest scale degree" and
//! "change the scale and rewrite the degrees coherently" — so the editor's music
//! theory and the evaluator's can never drift. Static + cheap (borrowed-static
//! data); no state, no I/O. Same boundary pattern as [`crate::reference`].

use merula::prelude::mode_table;
use serde::Serialize;

use crate::state::MerulaState;

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
#[arbor_rpc::handler]
fn merula_scales(_ctx: &MerulaState) -> Result<Vec<ScaleModeDto>, String> {
    Ok(mode_table()
        .iter()
        .map(|m| ScaleModeDto {
            name: m.name,
            aliases: m.aliases.to_vec(),
            intervals: m.intervals.to_vec(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use merula::prelude::mode_table;

    /// The catalogue is non-empty and includes `major` with the canonical
    /// diatonic interval set — a regression guard on the table the editor's
    /// scale-aware quick-fixes are driven from.
    #[test]
    fn catalogue_contains_major() {
        let table = mode_table();
        assert!(!table.is_empty(), "scale catalogue must not be empty");
        let major = table
            .iter()
            .find(|m| m.name == "major")
            .expect("major mode present");
        assert_eq!(major.intervals, [0, 2, 4, 5, 7, 9, 11].as_slice());
    }

    /// Every mode exposes exactly one octave of ascending, in-range intervals.
    #[test]
    fn intervals_are_one_octave_ascending() {
        for m in mode_table() {
            assert!(
                m.intervals.iter().all(|&i| (0..12).contains(&i)),
                "mode `{}` has an out-of-octave interval",
                m.name
            );
            assert!(
                m.intervals.windows(2).all(|w| w[0] < w[1]),
                "mode `{}` intervals must be strictly ascending",
                m.name
            );
        }
    }
}
