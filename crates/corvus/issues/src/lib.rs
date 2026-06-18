//! `corvus-issues` — the shared issue-tracker domain (Linear + Jira).
//!
//! The HTTP/GraphQL logic already lives in the keyring-free
//! `corvus-issue-tracker-{api,linear,jira}` crates, which take an injected
//! `Arc<dyn arbor_ipc::prelude::SessionProvider>`. This crate is the thin glue
//! both runtimes share:
//!
//! - **the shell** (`src-tauri`, in-process) builds the registry injecting
//!   `VaultSessionProvider` (keyring + OAuth, shell-side);
//! - **`corvus-be`** (out-of-process) injects `ChildSessionProvider`, which
//!   resolves credentials over the reverse channel back to the shell's vault.
//!
//! It deliberately holds **no** keyring / OAuth / `AppError` / Tauri — those stay
//! shell-side. Import via the [`prelude`].

pub mod build;
pub mod prelude;
pub mod registry;
pub mod types;
