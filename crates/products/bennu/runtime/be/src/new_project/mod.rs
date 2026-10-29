//! `new_project` — the New Project wizard's backend: pick a generator, fill its options, get a
//! project on disk.
//!
//! One concern per file:
//!
//! | File | Concern |
//! |---|---|
//! | `model.rs` | the wire types: fields, generators, the create request, the plan |
//! | `names.rs`, `target.rs` | validation of what the user typed, and of where it lands |
//! | `tools.rs` | which of `node`/`npm`/`cargo`/`git`/… are installed |
//! | `render.rs`, `templates/` | `{{placeholder}}` rendering of the data-file templates |
//! | `maven_gradle.rs`, `webapp.rs`, `rust.rs` | generators that write files locally |
//! | `spring.rs`, `quarkus.rs` | generators that download a skeleton from a fixed API |
//! | `node.rs` | generators that run the official scaffolding CLI |
//! | `http.rs`, `extract.rs` | the allow-listed network and the hardened unzip |
//! | `exec.rs`, `process.rs` | the staged run (execute a plan, stream progress, cancel, clean up) and the child-process runner |
//! | `catalog.rs`, `handlers.rs`, `prefs.rs`, `remote_meta.rs` | the generator list, the RPC surface, the remembered choices, the cached service metadata |
//!
//! ## One plan, one runner
//!
//! A generator never touches the destination. It turns the user's options into a [`model::Plan`]
//! — files to write, an archive to download, or a command to run — and `exec` executes any plan the
//! same way: **into a scratch folder beside the target**, then one rename. A failure or a cancel
//! deletes the scratch folder and the destination was never touched, which is what "the target is
//! never overwritten and a failed run leaves nothing behind" costs when the plan is the only thing
//! that varies.

mod catalog;
mod exec;
mod extract;
mod handlers;
mod http;
mod maven_gradle;
mod model;
mod names;
mod node;
mod prefs;
mod process;
mod quarkus;
mod remote_meta;
mod render;
mod rust;
mod spring;
mod target;
mod tools;
mod webapp;
