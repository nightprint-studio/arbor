//! File sources: `sample` (one-shot) and `audio` (long stem). In this pure
//! crate they are only **markers** — a `ControlMap` carrying the file path. The
//! actual decode/playback/mix is `arbor-grove-audio` (Fase 2).
//!
//! The one-shot vs. sustained distinction is realised by the audio engine; at
//! the pattern level both place the path marker once per cycle.

use crate::combinators::compose::pure;
use crate::control::ControlMap;
use crate::pattern::Pattern;

/// Load a file as a one-shot: a hap carrying the path marker each cycle.
pub fn sample(path: impl Into<String>) -> Pattern<ControlMap> {
    pure(ControlMap::source_file(path))
}

/// Load a long file (stem / take / ambience) as a path marker. The audio engine
/// plays it sustained from the start of the track.
pub fn audio(path: impl Into<String>) -> Pattern<ControlMap> {
    pure(ControlMap::source_file(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::TimeSpan;

    #[test]
    fn sample_marks_the_path() {
        let p = sample("drums/break.wav");
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.value.source_file.as_deref(), Some("drums/break.wav"));
    }
}
