//! `bennu-be` — the headless Java-editor / analysis backend process for Model D.
//!
//! The bennu twin of `tyto-be` / `merula-be`, on the slim path: it serves the bennu
//! domains (open_project / project_tree / read_file / capabilities / completion /
//! diagnostics) over framed-stdio IPC, drives the leaf analysis crates
//! (`bennu-project`, later `bennu-index` / `bennu-classpath` / `bennu-java` / …), and
//! has **no plugin host, no `NsHost`, no credentials/OAuth, and no pushed config** —
//! it resolves its own `bennu_config_dir()` / `bennu_data_dir()` once
//! `init_active_profile()` has run.
//!
//! Each domain's handler functions live in their own module here, auto-advertised via
//! `Hello` and auto-routed by the shell's broker. Phase 0 wires the real project
//! model (capability detection, pom parse, encoding/JDK detection, file tree,
//! decoded reads) plus the completion / diagnostics stubs that route through the
//! native intel provider — so the FE binds the whole contract from day one.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use bennu_core::prelude::BennuState;

// Self-test handlers (be_ping / be_echo) prove the framed-stdio handshake.
mod selftest;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one bennu
// domain. They self-register via `inventory`, so `arbor_rpc::registry()` collects
// them and `Hello` advertises them by name.
//
// The typed product config (`get/set_bennu_config`): the editor's per-project JDK /
// encoding overrides + defaults, owned out-of-process here.
mod config_cmds;
// Project model: `bennu_open_project` / `bennu_project_tree` / `bennu_read_file` —
// pom parse + capability detection + JDK/encoding detection + file tree + decoded
// reads, all driven through the leaf `bennu-project` crate.
mod project;
// Capabilities: `bennu_capabilities` — re-detect the Spike-D capability bitset for a
// project without re-opening it.
mod capabilities;
// Code-intel: `bennu_completion` / `bennu_diagnostics` — completion serves from the
// per-project index the `index_service` builds; diagnostics stay a stub for now.
mod intel;
// Refactor rename (docs §5 #10-12): `bennu_rename_plan` (preview) / `bennu_rename_apply`
// (edits) — best-effort, config-aware, off the per-project rename engine.
mod rename;
// The per-project index lifecycle: build the symbol index off-thread on open, cache
// the native provider, serve completion from it, and patch a single file on edit.
mod index_service;
// Config-graph input discovery: walk the project tree to find struts/spring/tiles files
// (`WebInputs`) for the config-graph build.
mod web_discovery;
// Build/run (docs §4 "il fondo"): `bennu_build` (mvn -q -o compile / javac fallback +
// error parser → structured diagnostics) / `bennu_run` (java -cp … streaming output) /
// `bennu_cancel_run`. Makes the Run/Debug buttons real + re-indexes target/classes.
mod build;

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, `bennu_config_dir()` /
    // `bennu_data_dir()` (which the domain modules read directly) silently resolve the
    // `default` profile instead of the one the launcher spawned us on, so a dev
    // launcher would read config/data from the wrong (or empty) profile.
    arbor_core::prelude::init_active_profile();

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime), in one
    // call. No `plugin_host` / `api_installer` — bennu-be loads no plugins in Phase 0
    // (like merula-be).
    let app = arbor_be::App::new(arbor_be::BackendIo::new());

    // The state every handler gets: event egress + the reverse channel (for host
    // round-trips like reveal-in-explorer). `Arc`-shared across the dispatcher + any
    // background workers (the future indexing thread).
    let state = Arc::new(BennuState::new(app.sink()).with_host_caller(app.host_caller()));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links. `inventory("")` covers them all — bennu-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects. No plugin host means the
    // `App`'s default post-`Hello` hook is a clean no-op (nothing to reload).
    if let Err(e) = app.run(dispatcher) {
        eprintln!("bennu-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
