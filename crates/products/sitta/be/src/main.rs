//! `sitta-be` — the headless file-explorer backend process for Model D.
//!
//! The sitta twin of `corvus-be` / `merula-be`: it serves the sitta domains over
//! framed-stdio IPC, loads **host-pure** Lua plugins (no product `arbor.*`
//! namespaces, no vetoable hooks — the wiring is `arbor-plugin-core`'s ready-made
//! host-pure pair), and has **no `NsHost`, no
//! credentials/OAuth, and no pushed config** — it resolves its own `sitta_*`
//! config / data dirs once `init_active_profile()` has run.
//!
//! A file manager keeps almost no backend domain state of its own: filesystem I/O
//! lives in `arbor-fs` (served by the shell's `platform` broker) and git-awareness
//! in `corvus-git`. The domains served here are the explorer git-awareness
//! ([`fs_git`]), the typed config ([`config_cmds`]) and the read-only workspace
//! queries ([`workspace`], a thin reader of corvus's `repos.json` / `workspaces.json`
//! for the Projects sidebar); each is an `#[arbor_rpc::handler]` module,
//! auto-advertised via `Hello` and auto-routed by the shell's broker.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use sitta_core::prelude::SittaState;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one sitta
// domain. They self-register via `inventory`, so `arbor_rpc::registry()` collects
// them and `Hello` advertises them by name.
mod selftest;
// Git awareness for the File Explorer — thin wrappers over `corvus_git::explorer`
// (the pure, shared git2 logic). Moved off `corvus-be` so the explorer's git works
// without the git client running.
mod fs_git;
// The typed global sitta config (`get/set_sitta_config`) — the explorer's own UX
// preferences, owned out-of-process here (the 4 window/OS-integration settings stay
// in the launcher config).
mod config_cmds;
// Read-only twin of corvus-be's workspace/registry queries (`list_workspaces` /
// `list_registry_repos`), parsing the same JSON directly so the Projects sidebar
// lists projects without spawning the git client.
mod workspace;
fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, the plugin dir and
    // any `sitta_config_dir()` / `sitta_data_dir()` would silently resolve the
    // `default` profile instead of the one the launcher spawned us on.
    arbor_core::prelude::init_active_profile();

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime) + the
    // host-pure plugin host: `plugin_host` builds the `PluginHost` (filtered to the
    // `sitta` product) + its headless `AppCtx` + the hook dispatcher; `api_installer`
    // publishes the host-pure `arbor.*` namespaces (no product namespaces). Plugins
    // under sitta's installed/ pool load on boot via the `App`'s post-`Hello` hook.
    let mut app = arbor_be::App::new(arbor_be::BackendIo::new());
    app.plugin_host("sitta", arbor_plugin_core::prelude::host_pure_hook_dispatcher);
    app.api_installer(arbor_plugin_core::prelude::host_pure_api_installer());

    // The state every handler gets: event egress + the reverse channel (for the
    // git-awareness wave that calls back into the shell). `Arc`-shared across the
    // dispatcher + any background workers.
    let state = Arc::new(SittaState::new(app.sink()).with_host_caller(app.host_caller()));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links (self-test today, the sitta domains as the waves land). `inventory("")`
    // covers them all — sitta-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects. The `App`'s post-`Hello`
    // hook boot-loads the host-pure plugins from sitta's installed/ pool.
    if let Err(e) = app.run(dispatcher) {
        eprintln!("sitta-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
