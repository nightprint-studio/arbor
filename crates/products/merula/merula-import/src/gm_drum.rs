//! General MIDI percussion key → merula sound name.
//!
//! GM channel 10 assigns a fixed instrument to each note number (35 = acoustic
//! bass drum, 38 = acoustic snare, …). merula drum samples follow the
//! Dirt-Samples naming (`bd`, `sn`, `hh`, …), so importing a MIDI drum part
//! means translating each key to the closest standard name. Unknown keys fall
//! back to `perc` rather than being dropped — a faithful import keeps the hit.

/// Map a GM percussion key (channel 10 note number) to a merula sound name.
pub fn sound_for_key(key: i32) -> &'static str {
    match key {
        35 | 36 => "bd",      // bass drum
        37 => "rim",          // side stick / rimshot
        38 | 40 => "sn",      // snare (acoustic / electric)
        39 => "cp",           // hand clap
        41 | 43 | 45 => "lt", // low / low-mid / high-floor tom → low tom
        47 | 48 | 50 => "mt", // mid / hi-mid / high tom → mid/high tom
        42 | 44 => "hh",      // closed / pedal hi-hat
        46 => "oh",           // open hi-hat
        49 | 57 => "cr",      // crash 1 / crash 2
        51 | 59 => "rd",      // ride 1 / ride 2
        52 => "cr",           // china → crash-ish
        53 => "rd",           // ride bell
        54 => "tb",           // tambourine
        55 => "cr",           // splash → crash-ish
        56 => "cb",           // cowbell
        70 | 69 => "sh",      // maracas / shaker
        75..=77 => "cl",      // claves / woodblocks
        _ => "perc",
    }
}
