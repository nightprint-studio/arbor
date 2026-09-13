//! `merula-be`'s answer to `__memory` — what the live session keeps decoded.
//!
//! The bulk is audio: every sample an arrangement has referenced is decoded to `f32` and stays
//! resident for as long as the session runs, because the real-time callback cannot wait on a
//! decode. That figure is published by the audio thread (`resident_samples`), since the registry
//! itself lives where nothing else may touch it. merula-be hosts no plugins.

use arbor_be::prelude::{MemoryItem, MemoryReport, PROCESS_SCOPE};
use merula_core::prelude::{resident_samples, with_latest, MerulaState};

/// Everything this backend can say about its own memory.
pub(crate) fn report(state: &MerulaState) -> MemoryReport {
    let (samples, bytes) = resident_samples();
    let mut items = vec![MemoryItem::exact(
        PROCESS_SCOPE,
        "Decoded samples, resident for playback",
        samples as usize,
        bytes as usize,
    )];

    let instruments = state
        .session()
        .as_ref()
        .map(|s| s.loaded.lock().map(|l| l.len()).unwrap_or(0))
        .unwrap_or(0);
    items.push(MemoryItem::counted(PROCESS_SCOPE, "Instruments loaded", instruments));

    if let Some((tracks, scenes)) = with_latest(|l| (l.tracks.tracks.len(), l.scenes.len())) {
        items.push(MemoryItem::counted(PROCESS_SCOPE, "Tracks of the last evaluation", tracks));
        items.push(MemoryItem::counted(PROCESS_SCOPE, "Scenes of the last evaluation", scenes));
    }

    MemoryReport { items }
}
