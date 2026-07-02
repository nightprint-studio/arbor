//! `tyto-be` — the headless screen-recorder backend process for Model D.
//!
//! The tyto twin of `sitta-be` / `merula-be`: it serves the tyto domains over
//! framed-stdio IPC, loads **host-pure** Lua plugins (no product `arbor.*`
//! namespaces, no vetoable hooks — see [`plugin`]), and has **no `NsHost`, no
//! credentials/OAuth, and no pushed config** — it resolves its own `tyto_*`
//! config / data dirs once `init_active_profile()` has run.
//!
//! The tyto domains (sources / session / region / library / config) are thin RPC
//! wrappers over a shared recording engine. Each is an `#[arbor_rpc::handler]`
//! module, auto-advertised via `Hello` and auto-routed by the shell's broker.
//! Today every capture handler is a stub (the recording engine is a later wave):
//! it returns empty lists or a "capture backend not available" error, so the
//! frontend degrades gracefully instead of showing fake devices.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use tyto_core::prelude::TytoState;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one tyto
// domain. They self-register via `inventory`, so `arbor_rpc::registry()` collects
// them and `Hello` advertises them by name.
mod selftest;
// The typed product tyto config (`get/set_tyto_config`) — the recorder's own
// capture/encoding/output defaults, owned out-of-process here (the opt-in
// OS-global open shortcut stays in the launcher config).
mod config_cmds;
// Capture-target enumeration (monitors / windows / audio inputs) — replaces the
// frontend mock's device fixtures.
mod sources;
// Recording session lifecycle (start / stop / pause / screenshot / poll) —
// replaces the frontend mock's local timer + synthetic captures.
mod session;
// Region-of-interest selection (CSS→physical-pixel resolve) — replaces the
// frontend mock's drag math.
mod region;
// The saved-captures library (list / rename / remove / clear / reveal / open) —
// replaces the frontend mock's in-memory list.
mod library;
// Host-pure plugin-host wiring (hook dispatcher + `arbor.*` base installer).
mod plugin;
// The capture engine (scap capture, windows-capture enumeration, ffmpeg encode,
// cpal mic) that the domains above drive. Native deps live here, never in the shell.
mod capture;

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, the plugin dir and
    // any `tyto_config_dir()` / `tyto_data_dir()` would silently resolve the
    // `default` profile instead of the one the launcher spawned us on.
    arbor_core::prelude::init_active_profile();

    // Provision the ffmpeg binary the encoder needs, off-thread + best-effort:
    // ffmpeg-sidecar downloads it once to its cache if absent (needs network on
    // first run). A failure here isn't fatal — recording just errors until an
    // ffmpeg is on PATH / bundled beside the binary.
    std::thread::spawn(|| {
        if let Err(e) = ffmpeg_sidecar::download::auto_download() {
            eprintln!("tyto-be: ffmpeg auto-provision skipped: {e}");
        }
    });

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime) + the
    // host-pure plugin host: `plugin_host` builds the `PluginHost` (filtered to the
    // `tyto` product) + its hook dispatcher; `api_installer` publishes the host-pure
    // `arbor.*` namespaces (no product namespaces). Plugins under tyto's installed/
    // pool load on boot via the `App`'s post-`Hello` hook.
    let mut app = arbor_be::App::new(arbor_be::BackendIo::new());
    app.plugin_host("tyto", plugin::tyto_hook_dispatcher);
    app.api_installer(plugin::tyto_be_api_installer());

    // The state every handler gets: event egress + the reverse channel (reveal /
    // open a saved capture). `Arc`-shared across the dispatcher + the recording
    // engine thread.
    let state = Arc::new(TytoState::new(app.sink()).with_host_caller(app.host_caller()));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links. `inventory("")` covers them all — tyto-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects. The `App`'s post-`Hello`
    // hook boot-loads the host-pure plugins from tyto's installed/ pool.
    if let Err(e) = app.run(dispatcher) {
        eprintln!("tyto-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
