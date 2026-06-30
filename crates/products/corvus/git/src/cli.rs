//! [`GitCli`] — explicit git-binary invocation.
//!
//! The shell resolves the git program (PATH / configured / portable) and the
//! headless backend resolves its own; each constructs a `GitCli` and passes it
//! in. No global state lives here, so there is nothing to synchronize across the
//! shell ↔ `corvus-be` process boundary.

use std::path::PathBuf;
use std::process::Command;

use arbor_process_ext::prelude::NoWindowExt;

/// Builds pre-configured git `Command`s (no console window on Windows).
#[derive(Debug, Clone)]
pub struct GitCli {
    program: PathBuf,
}

impl GitCli {
    /// Invoke this exact git program.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self { program: program.into() }
    }

    /// Use the resolved program if present, else `git` on `PATH`.
    pub fn from_optional(program: Option<PathBuf>) -> Self {
        Self { program: program.unwrap_or_else(|| PathBuf::from("git")) }
    }

    /// A git `Command` with the console window suppressed on Windows.
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.no_window();
        c
    }
}
