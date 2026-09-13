//! `tyto-be`'s answer to `__memory` — what a recording holds while it runs.
//!
//! Tyto keeps nothing between recordings: sources, the library and screenshots are computed per call
//! and written straight to disk. While a recording runs it holds frames — the newest capture, and the
//! ones queued for the sink. The plugin VMs are listed by the runtime (`arbor-be`), not here.

use arbor_be::prelude::{MemoryItem, MemoryReport, PROCESS_SCOPE};

use crate::capture::ENGINE;

/// The most frames a sink queues behind the newest one: the video sink's channel into ffmpeg holds
/// eight (`video.rs`), and the frame writer's job queue is two per worker with at most four workers
/// (`frames.rs`). A bound, not a measurement — the queues are usually far emptier.
const QUEUED_FRAMES_AT_MOST: usize = 8;

/// Everything this backend can say about its own memory.
pub(crate) fn report() -> MemoryReport {
    let Some(rec) = ENGINE.footprint() else {
        return MemoryReport { items: vec![MemoryItem::counted(PROCESS_SCOPE, "Recordings running", 0)] };
    };
    MemoryReport {
        items: vec![
            MemoryItem::exact(PROCESS_SCOPE, "Newest captured frame", 1, rec.frame_bytes),
            MemoryItem::estimate(
                PROCESS_SCOPE,
                "Frames queued for the encoder, at most",
                QUEUED_FRAMES_AT_MOST,
                QUEUED_FRAMES_AT_MOST * rec.frame_bytes,
            ),
            MemoryItem::counted(PROCESS_SCOPE, "Frames recorded so far", rec.frames as usize),
        ],
    }
}
