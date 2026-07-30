//! `picus-be` — the headless SQL-studio backend process for Model D.
//!
//! The picus twin of `bennu-be` / `tyto-be`, on the slim path: it serves the picus
//! domains over framed-stdio IPC, drives the leaf crates (the database providers,
//! then the script half: parse / inventory / analyse / emit / rewrite), and has **no
//! plugin host, no pushed config, and no stored password** — it resolves its own
//! `picus_config_dir()` / `picus_data_dir()` once `init_active_profile()` has run,
//! and asks the shell's credential broker for a secret at the moment of use.
//!
//! Each domain's handler functions live in their own module here, auto-advertised
//! via `Hello` and auto-routed by the shell's broker. Served today: the typed
//! product config, the per-engine descriptors, and the whole database half —
//! connections, schema, scrolling results, statement execution and cancellation,
//! against PostgreSQL. The script half (parse / inventory / analyse / emit /
//! rewrite) lands in the following waves against the same `PicusState`.
//!
//! Two rules this binary must keep as it grows (see `docs/picus-design.md`):
//!
//! * **No language model, anywhere in the flow.** Generation is structured input →
//!   model → per-dialect emission. Deterministic, testable, diffable.
//! * **No ambient dialect.** The dialect is a property of the folder being written;
//!   every emit/parse/rewrite entry point takes it explicitly.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use picus_core::prelude::PicusState;
use picus_db_api::prelude::DbProviderRegistry;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one picus
// domain. They self-register via `inventory`, so `arbor_rpc::registry()` collects
// them and `Hello` advertises them by name.
//
// Self-test handlers (be_ping / be_echo) prove the framed-stdio handshake.
mod selftest;
// The typed product config (`get/set_picus_config`): the studio's encoding
// fallbacks, write guards, emission defaults and query row limit, owned
// out-of-process here.
mod config_cmds;
// Connection lifecycle: the configured list (persisted, never with a password) plus
// open / close / test against a live server.
mod connections;
// Schema reads: the browser tree and one relation in full. Its *rows* are a read
// like any other and live in `query`.
mod schema;
// Statement execution, the held results a read leaves behind (windows, exact count,
// close) and server-side cancellation.
mod query;
// The per-engine descriptors the UI renders from — including engines with no
// driver, which is how Oracle stays a first-class script engine.
mod providers;
// Password resolution over the shell's credential broker. The only module that
// knows the mechanism; the driver crates see a trait.
mod secrets;
// Deterministic SQL generation — one dialect-free model in, one statement per
// destination out. Thin wrappers over `picus-emit`, which owns the golden tests.
mod emit;
// Opening a repository of scripts: propose what it is, let the user correct it,
// write `.arbor/picus/project.toml` only once they have confirmed.
mod project;
// Reading that repository and holding it: every script decoded once, then parsed,
// indexed and measured against the fourteen consistency rules.
mod scripts;
// Where a generated block goes in a destination file, and which bytes it replaces
// when Picus has written into that file before. Pure.
mod placement;
// The two calls that write: a preview that returns the exact bytes, and an apply
// that refuses if any of them moved in between.
mod apply;
// The SQL abbreviation expander, joined to a live connection: the language is a
// foundation crate, this supplies the schema, the dialect and the emitter.
mod abbrev;
// The columns of a table no connected database knows, read from the scripts.
mod columns;
// What the scripts already say about these rows and this version range.
mod reconcile;
// Named sets of destinations — "where a change like this always goes".
mod destinations;
// The three abbreviations the two engines spell differently — merge, alter, loop.
mod abbrev_render;
// Writing a result grid's changed cells back — the one place Picus issues DML the
// user did not read first, which is why it refuses more than it accepts.
mod edits;
// The syntax tree of one script, for the AST panel — `arbor-syntax` pointed at
// Picus's grammar and Picus's already-decoded text.
mod ast;
// Structural search and replace across the repository: find → preview → apply,
// with the same digest guarantee a generation has.
mod restructure;
// Where one statement ends and the next begins, so Run can execute exactly one.
mod statements;
// Which relation a result's rows came from, and whether it is a view — the answer
// the export, the cell editing and the large-object read all need, taken from the
// parser rather than approximated from the text.
mod source_relation;
// What every session on the server is doing, and who is blocked behind whom.
mod activity;
// Values bound to a statement's placeholders rather than spliced into its text.
mod binds;
// What depends on what — foreign keys, view bodies, triggers, sequence defaults.
mod depends;
// The plan for a statement: what the server says it will do, or what it just did.
mod plan;
// Explicit transactions — open one, look at what you did, then decide.
mod tx;

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, any
    // `picus_config_dir()` / `picus_data_dir()` would silently resolve the
    // `default` profile instead of the one the launcher spawned us on.
    arbor_core::prelude::init_active_profile();

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime). Slim
    // path: no plugin host and no API installer.
    //
    // When Picus does want plugins (custom emission rules and naming schemes are
    // the obvious candidates), do NOT copy sitta-be/tyto-be's `plugin.rs` a third
    // time — that file already carries the note to promote the host-pure wiring
    // into a shared `arbor-plugin-be` crate first.
    let app = arbor_be::App::new(arbor_be::BackendIo::new());

    // The database engines this binary links. THE place where "which engines can be
    // connected to" is decided — everything downstream reads the registry, so adding
    // Oracle later is one more `.with(...)` here plus its crate, and no edit
    // anywhere else. An engine absent from the registry is still fully supported on
    // the script side; that is exactly Oracle's situation today.
    let providers =
        DbProviderRegistry::new().with(Arc::new(picus_db_postgres::prelude::PostgresProvider::new()));

    // The state every handler gets: event egress, the reverse channel, the engine
    // registry and the live-session pool. The channel is load-bearing for Picus —
    // the product stores no password, so a connection's secret is resolved through
    // the shell's credential broker at the moment of use. `Arc`-shared across the
    // dispatcher + any background workers.
    let state = Arc::new(
        PicusState::new(app.sink())
            .with_host_caller(app.host_caller())
            .with_providers(providers),
    );

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links. `inventory("")` covers them all — picus-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects.
    if let Err(e) = app.run(dispatcher) {
        eprintln!("picus-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
