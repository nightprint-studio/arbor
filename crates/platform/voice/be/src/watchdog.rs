//! Idle exit. A dictation backend that nobody is using must cost nothing: after
//! `idle_minutes` without any request, recording, transcription, download or armed wake word,
//! the models are dropped and the process exits. The shell notices the disconnect and the next
//! use respawns it.

use std::time::Duration;

use crate::{engine, models_cmds, session, shared};

/// How often the watchdog looks. A sleeping thread costs no CPU.
const TICK: Duration = Duration::from_secs(20);

pub fn spawn() {
    let _ = std::thread::Builder::new().name("voice-watchdog".into()).spawn(|| loop {
        std::thread::sleep(TICK);
        let state = shared::get();
        if session::busy() || models_cmds::downloading() {
            state.touch();
            continue;
        }
        let limit_ms = u64::from(state.config().idle_minutes) * 60_000;
        // Free the (large) models first: even if the process lingers a bit, the memory is back.
        if state.idle_ms() > limit_ms / 2 {
            engine::unload_all();
        }
        if state.idle_ms() > limit_ms {
            eprintln!("voice-be: idle, exiting");
            std::process::exit(0);
        }
    });
}
