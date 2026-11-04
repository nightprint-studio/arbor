//! `hirundo-be` — the headless HTTP-client backend process for Model D.
//!
//! Hirundo is the Bennu suite's Postman/Bruno: collections of requests kept as readable files in the repo
//! (`.hirundo/`), environments with **types** (local / dev / staging / prod and the user's own, each with a
//! colour, a protection and a set of variables), auth inherited down collection → folder → request, cURL in
//! both directions, and a sender that shows exactly what it sent.
//!
//! Like `tyto-be` it hosts no plugins and has no pushed config: it resolves everything from the active
//! profile and from the project path each call names. Secrets are never held here — a variable marked
//! `@secret` is asked of the shell's keyring over the reverse channel, per request.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use hirundo_core::prelude::HirundoState;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s of one domain. They self-register via
// `inventory`, so `Hello` advertises them by name.
mod charset;
mod context;
mod cookie_store;
mod convert;
mod envs;
mod httprun;
mod httpfile;
mod selftest;
mod openapi;
mod send;
mod soap;
mod sse;
mod config_cmds;
mod project_state;
mod runs;
mod schedules;
mod tests_run;
mod tree;

fn main() {
    // Seed the active profile FIRST: anything profile-scoped resolves under the launcher's profile.
    arbor_core::prelude::init_active_profile();

    let app = arbor_be::App::new(arbor_be::BackendIo::new());
    let state = Arc::new(HirundoState::new(app.sink()).with_host_caller(app.host_caller()));

    let dispatcher = arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    if let Err(e) = app.run(dispatcher) {
        eprintln!("hirundo-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    let _ = io::stderr().flush();
}
