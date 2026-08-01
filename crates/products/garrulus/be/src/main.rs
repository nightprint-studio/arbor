//! `garrulus-be` — the headless note-vault backend process for Model D.
//!
//! The garrulus twin of `sitta-be` / `picus-be`: it serves the garrulus domains
//! over framed-stdio IPC and has **no `NsHost`, no credentials/OAuth and no pushed
//! config** — it resolves its own per-profile config / vault registry once
//! `init_active_profile()` has run.
//!
//! What it owns, and nothing else does: the open vault, the index over it, the
//! sync remote, and the filesystem watcher that notices when something else
//! (a pull, the other PC, Obsidian on the same folder) changes a note under it.
//!
//! **stdout is the protocol channel** — all logs go to stderr.
//!
//! ## Handler shape
//!
//! Every handler here is thin by construction: validate the arguments, call into
//! `garrulus-vault` / `-index` / `-sync` through [`vault_io`], map the error to a
//! `String` (the error string IS the wire contract), and fire the plugin hook
//! **after every lock guard has been dropped** — see `garrulus_core::state`'s
//! locking discipline. A handler that needed a paragraph of logic of its own is a
//! handler whose logic belongs in a leaf crate.
//!
//! ## Hooks fired
//!
//! `garrulus:vault_opened`, `garrulus:vault_closed`, `garrulus:note_created`,
//! `garrulus:note_saved`, `garrulus:note_renamed`, `garrulus:note_deleted`,
//! `garrulus:sync_started`, `garrulus:sync_done`, `garrulus:sync_conflict`,
//! `garrulus:type_applied` — every one of them a constant in
//! [`garrulus_core::hooks`], never a literal at the fire site. Their ctx schemas
//! are declared in the hook catalog, which is what `arbor.hooks.describe()`
//! answers from — a fire whose payload drifts from that entry is a silent lie to
//! every plugin author, so the two are edited together.
//!
//! The product namespace is what keeps `garrulus:note_saved` apart from corvus's
//! `corvus:note_saved` (a *git* note on a commit, payload
//! `{tab_id, commit_oid, namespace}`). That is why neither side carries a
//! disambiguating infix in the event name: the collision is structurally
//! impossible rather than avoided by hand.
//!
//! They fire through the state's `HookDispatcher`, which `main` builds from this
//! backend's own **host-pure** plugin host: plugins targeting `garrulus` (or
//! targeting nothing) load from the profile's pool and see the platform `arbor.*`
//! namespaces. There is deliberately **no garrulus product namespace** — the vault
//! verbs are RPC methods, not Lua API, so a plugin observes the vault through these
//! hooks rather than driving it.

use std::io::{self, Write};
use std::sync::{Arc, OnceLock};

use garrulus_core::prelude::{hooks, GarrulusState};

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one
// garrulus domain. They self-register via `inventory`, so `arbor_rpc::registry()`
// collects them and `Hello` advertises them by name.
mod selftest;
// The typed global garrulus config (`get/set_garrulus_config`) — device name, sync
// cadence, editor prefs. Per-vault settings live inside the vault instead.
mod config_cmds;
// Open / create / list / close a vault, and the index rebuild that goes with it.
mod vault;
// Read / write / create / rename / delete a note. Deletion goes to the vault's own
// `.arbor/garrulus/trash/`, not straight out of existence.
mod note;
// Note types: list them, apply one to an existing note, render a type's template.
mod types;
// Everything the index answers: search, quick switch, backlinks, vault problems.
mod search;
// The sync seam: state, sync/pull/push, conflicts and per-note history. The
// background never calls anything here except `probe_state`.
mod sync;
// Where the vault syncs to: configure / clear / test a destination, create one
// through the shell's git provider, and install the one the registry remembers.
mod remote;
// The read-only background probe on `arbor-scheduler` — the only thing that
// touches the remote without a click, and it can only ever read.
mod probe;
// The vault trash: list, restore, purge, empty. Thin over `garrulus-vault`.
mod trash;
// The `notify` watcher over the open vault, emitting debounced
// `garrulus:vault-changed` events from its own thread.
mod watch;

// Shared, non-handler modules.
//
// The single place this binary touches the vault on disk: the seam onto
// `garrulus-vault`'s I/O plus the path guarding every note-addressing handler
// needs. If the vault crate's API differs, this is the only file to edit.
mod vault_io;
// A pure frontmatter key setter, used by `apply_type`. Lives here only until
// `garrulus-ast` owns it — see the module's own note.
mod frontmatter;

/// The process's one [`GarrulusState`], as an owned handle.
///
/// A handler is handed `&GarrulusState`, but two things genuinely need an owned
/// `Arc`: the credential provider a `SyncRemote` closes over (it reaches the
/// shell's broker through the state's reverse channel, and outlives the call that
/// built it), and the background probe's action (`Fn + Send + Sync + 'static`).
/// `main` is the only place that has the `Arc`, so `main` is the place that
/// publishes it — better one named handle here than an ad-hoc static in each of
/// the two modules that need one.
static STATE: OnceLock<Arc<GarrulusState>> = OnceLock::new();

/// The process's state handle.
///
/// The error arm is unobservable from a handler: `main` publishes the state
/// before the serve loop starts, and there is no other way in. It exists so the
/// failure stays a `Result` the caller reports rather than an `unwrap` that takes
/// the backend down.
pub(crate) fn state_arc() -> Result<Arc<GarrulusState>, String> {
    STATE
        .get()
        .cloned()
        .ok_or_else(|| "garrulus-be: the state handle has not been published yet".to_string())
}

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, the garrulus config
    // and the vault registry would silently resolve the `default` profile instead
    // of the one the launcher spawned us on.
    arbor_core::prelude::init_active_profile();

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime) + the
    // host-pure plugin host: `plugin_host` builds the `PluginHost` (filtered to the
    // `garrulus` product — a plugin loads here if its manifest targets `garrulus`
    // or targets nothing), its headless `AppCtx`, the hook dispatcher and the
    // trigger engine; `api_installer` publishes the `arbor.*` namespaces.
    //
    // **No garrulus product namespace on purpose, not by omission.** The vault
    // verbs (open, save, sync, apply a type) are RPC methods the frontend calls,
    // not Lua API — a plugin observes the vault through the ten hooks below rather
    // than driving it. The day a plugin needs to *act* on the vault, this stops
    // being host-pure: a real `LuaApiInstaller` here passes the product's
    // namespaces as `extra`, the way corvus does.
    let mut app = arbor_be::App::new(arbor_be::BackendIo::new());
    // `hooks::NS` and not the literal "garrulus": the product id given here is the
    // implicit prefix a Lua subscriber gets (`arbor.events.on("note_saved", …)`
    // resolves to `garrulus:note_saved`), so it must be the same string the hook
    // constants are built from — not a second copy that can drift.
    app.plugin_host(hooks::NS, arbor_plugin_core::prelude::host_pure_hook_dispatcher);
    app.api_installer(arbor_plugin_core::prelude::host_pure_api_installer());

    // The state every handler gets: event egress, the reverse channel (the sync
    // engine asks the shell's credential broker through it), the hook broker built
    // just above — without `with_hooks` the state keeps its default empty
    // dispatcher and all ten `fire_hook` call sites stay silent no-ops — and the
    // three long-lived pieces: vault, index, remote.
    let state = Arc::new(
        GarrulusState::new(app.sink())
            .with_hooks(app.hooks())
            .with_host_caller(app.host_caller()),
    );
    // Published before anything can ask for it; see `STATE`.
    let _ = STATE.set(Arc::clone(&state));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links. `inventory("")` covers them all — garrulus-be links only its own.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Post-`Hello` startup, and it has TWO jobs.
    //
    // Everything here runs strictly after the `Hello` frame is on the wire, which
    // is the whole point of `on_ready`: the sync probe's output is
    // `garrulus:sync-state` events and a plugin's `on_plugin_load` can emit too,
    // and an event that precedes the handshake makes the shell reject the
    // connection (landmine #4 in `docs/backend-architecture.md`).
    //
    // The trap: overriding `on_ready` **replaces** the `App`'s default, and that
    // default's entire body is the plugin reload. Register only the probe and the
    // host loads zero plugins — the dispatcher fans out to an empty list, so every
    // hook is a no-op again, with nothing logged anywhere to say so. Hence the
    // reload is spelled out here. (garrulus-be is the first backend to combine a
    // plugin host with an `on_ready` override; if a second one appears, the fix is
    // to make `App::on_ready` additive rather than to copy this block.)
    //
    // On its own thread because `serve_stdio` runs `on_ready` inline *before* it
    // starts reading frames: a slow plugin load — or one that re-enters the shell
    // through a host_call from `on_load` — would stall the read loop the shell is
    // already waiting on, and downstream the Garrulus window would never open.
    let probe_rt = app.runtime_handle();
    let plugin_host = app.plugin_host_handle();
    app.on_ready(move || {
        std::thread::spawn(move || {
            let mut host = plugin_host.lock().unwrap_or_else(|p| p.into_inner());
            if let Err(e) = host.reload() {
                eprintln!("garrulus-be: plugin reload failed: {e}");
            }
            host.start_all_schedulers();
        });
        probe::start(probe_rt);
    });

    // Serve over framed stdio until the shell disconnects.
    if let Err(e) = app.run(dispatcher) {
        eprintln!("garrulus-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited. Stop the watcher thread so the process can go.
    watch::stop();
    let _ = io::stderr().flush();
}
