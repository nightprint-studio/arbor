//! `merula-be` — the headless audio / live-coding backend process for Model D.
//!
//! The merula twin of `corvus-be`, but slimmer: it serves the merula domains
//! (eval / transport / render / packs / sounds / scenes / …) over framed-stdio
//! IPC, owns its **own** dedicated audio session thread (the real-time path is
//! sacred), and has **no plugin host, no `NsHost`, no credentials/OAuth, and no
//! pushed config** — it resolves its own `merula_config_dir()` / `merula_data_dir()`
//! once `init_active_profile()` has run.
//!
//! Each domain's handler functions live in their own module here, auto-advertised
//! via `Hello` and auto-routed by the shell's broker; this skeleton wires the
//! scaffold + the self-test method set, and every domain module is an empty stub a
//! later wave fills.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use merula_core::prelude::MerulaState;

// Foundation: the canonical state + audio substrate (state / session / control /
// events / audio_thread / config-type / pack+alias read helpers) lives in
// merula-core now; this binary keeps only the self-test handlers that prove the
// handshake plus the domain handler modules below.
mod selftest;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one merula
// domain (ported from `src-tauri/src/merula/*` + `commands/merula_*`). They
// self-register via `inventory`, so `arbor_rpc::registry()` collects them and
// `Hello` advertises them. Empty stubs for now; a later wave fills each in.
mod materialize;
mod query;
mod scenes;
mod sounds;
mod format;
mod scales;
mod reference;
mod packs;
mod models;
mod libraries;
mod project;
mod config_cmds;
mod fstate;
mod eval;
mod audio_cmds;
mod devices;
mod jobs;
mod render;
mod packs_download;
mod models_download;
mod importers;
mod libraries_sync;

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, `merula_config_dir()`
    // / `merula_data_dir()` (which the domain modules read directly) silently
    // resolve the `default` profile instead of the one the launcher spawned us on,
    // so a dev launcher would read config/data from the wrong (or empty) profile.
    arbor_core::prelude::init_active_profile();

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime), in one
    // call. No `plugin_host` / `api_installer` — merula-be loads no plugins.
    let app = arbor_be::App::new(arbor_be::BackendIo::new());

    // The state every handler gets: event egress + the reverse channel (for the
    // job-driving domains). `Arc`-shared across the dispatcher + any background
    // workers (the audio thread, render jobs).
    let state = Arc::new(MerulaState::new(app.sink()).with_host_caller(app.host_caller()));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links (self-test today, the merula domains as the waves land). `inventory("")`
    // covers them all — merula-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects. No plugin host means the
    // `App`'s default post-`Hello` hook is a clean no-op (nothing to reload).
    if let Err(e) = app.run(dispatcher) {
        eprintln!("merula-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
