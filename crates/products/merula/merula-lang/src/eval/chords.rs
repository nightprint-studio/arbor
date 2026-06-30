//! Chord interval tables for the mini-notation `'name` postfix.
//!
//! The pattern crate owns scales; chords live here because they are a language
//! surface feature (`n(c4'min7)`). Intervals are semitones above the root, per
//! `design/merula/mini-notation.md`. `m` is an alias for `min`.

/// Semitone offsets above the root for a chord name, or `None` if unknown.
pub fn chord_intervals(name: &str) -> Option<&'static [i32]> {
    Some(match name {
        "maj" => &[0, 4, 7],
        "min" | "m" => &[0, 3, 7],
        "dim" => &[0, 3, 6],
        "aug" => &[0, 4, 8],
        "sus2" => &[0, 2, 7],
        "sus4" => &[0, 5, 7],
        "5" => &[0, 7],
        "6" => &[0, 4, 7, 9],
        "min6" | "m6" => &[0, 3, 7, 9],
        "add9" => &[0, 4, 7, 14],
        "7" => &[0, 4, 7, 10],
        "maj7" => &[0, 4, 7, 11],
        "min7" | "m7" => &[0, 3, 7, 10],
        "dim7" => &[0, 3, 6, 9],
        "m7b5" => &[0, 3, 6, 10],
        "minMaj7" | "mMaj7" => &[0, 3, 7, 11],
        "aug7" => &[0, 4, 8, 10],
        "9" => &[0, 4, 7, 10, 14],
        "maj9" => &[0, 4, 7, 11, 14],
        "min9" | "m9" => &[0, 3, 7, 10, 14],
        "11" => &[0, 4, 7, 10, 14, 17],
        "min11" | "m11" => &[0, 3, 7, 10, 14, 17],
        "13" => &[0, 4, 7, 10, 14, 21],
        "maj13" => &[0, 4, 7, 11, 14, 21],
        "min13" | "m13" => &[0, 3, 7, 10, 14, 21],
        _ => return None,
    })
}
