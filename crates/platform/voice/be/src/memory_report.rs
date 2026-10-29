//! `voice-be`'s answer to `__memory`: what the process holds, so the process monitor can say
//! "Whisper, 1.1 GB, idle" instead of an anonymous number.

use arbor_be::prelude::{MemoryItem, MemoryReport, PROCESS_SCOPE};

use crate::{engine, session};

pub(crate) fn report() -> MemoryReport {
    let s = session::status();
    let label = match s.state {
        "listening" => "State: listening",
        "transcribing" => "State: transcribing",
        _ => "State: idle",
    };
    MemoryReport {
        items: vec![
            MemoryItem::counted(PROCESS_SCOPE, label, 0),
            MemoryItem::counted(PROCESS_SCOPE, "Speech models in memory", engine::loaded()),
            MemoryItem::counted(PROCESS_SCOPE, "Wake word armed", usize::from(s.armed)),
        ],
    }
}
