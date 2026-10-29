//! `voice-be` - local speech-to-text as a headless, idle-exiting process (Model D).
//!
//! Platform-level: any product window can borrow it (`ensure_product_backend("voice")`). It
//! starts on first use and exits after `idle_minutes` of inactivity, so at rest it costs no
//! memory and no CPU. Microphone capture (cpal) and transcription (whisper.cpp, feature
//! `whisper`) run on their own threads, never on a runtime worker.
//!
//! **stdout is the protocol channel** - all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use voice_core::prelude::VoiceState;

mod capture;
mod diag;
mod engine;
mod handlers;
mod jobs;
mod live;
mod memory_report;
mod models_cmds;
mod priority;
mod selftest;
mod session;
mod shared;
mod watchdog;

fn main() {
    // Seed the active profile FIRST: the config path depends on it.
    arbor_core::prelude::init_active_profile();

    let app = arbor_be::App::new(arbor_be::BackendIo::new());
    let state = Arc::new(VoiceState::new(app.sink()));
    shared::init(Arc::clone(&state));
    jobs::init(app.host_caller());
    watchdog::spawn();

    let dispatcher = arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle())
        .inventory("")
        .memory(|_state| memory_report::report());

    if let Err(e) = app.run(dispatcher) {
        eprintln!("voice-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    let _ = io::stderr().flush();
}
