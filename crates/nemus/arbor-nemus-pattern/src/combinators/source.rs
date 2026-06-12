//! File sources: `sample` (one-shot) and `audio` (long stem). In this pure
//! crate they are only **markers** — a `ControlMap` carrying the file path. The
//! actual decode/playback/mix is `arbor-nemus-audio` (Fase 2).
//!
//! Both place the path marker once per cycle; they differ only in the
//! [`SourceKind`] they stamp, which the audio engine reads to decide one-shot
//! vs. sustained playback.

use crate::combinators::compose::pure;
use crate::control::{ControlMap, SourceKind};
use crate::pattern::Pattern;

/// Build a file-source marker carrying its playback kind.
fn file_source(path: impl Into<String>, kind: SourceKind) -> Pattern<ControlMap> {
    let mut c = ControlMap::source_file(path);
    c.source_kind = Some(kind);
    pure(c)
}

/// Load a file as a one-shot: a hap carrying the path marker each cycle.
pub fn sample(path: impl Into<String>) -> Pattern<ControlMap> {
    file_source(path, SourceKind::OneShot)
}

/// Load a long file (stem / take / ambience) as a path marker. The audio engine
/// plays it sustained from the start of the track.
pub fn audio(path: impl Into<String>) -> Pattern<ControlMap> {
    file_source(path, SourceKind::Sustained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::TimeSpan;

    #[test]
    fn sample_marks_the_path_one_shot() {
        let p = sample("drums/break.wav");
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.value.source_file.as_deref(), Some("drums/break.wav"));
        assert_eq!(h.value.source_kind, Some(SourceKind::OneShot));
    }

    #[test]
    fn audio_marks_the_path_sustained() {
        let p = audio("vox/take.wav");
        let h = &p.query(TimeSpan::cycle(0))[0];
        assert_eq!(h.value.source_file.as_deref(), Some("vox/take.wav"));
        assert_eq!(h.value.source_kind, Some(SourceKind::Sustained));
    }
}
