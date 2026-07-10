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

    /// A git `Command` with the console window suppressed on Windows and every
    /// interactive credential path disabled.
    ///
    /// Arbor always injects its own HTTPS auth (`-c http.<host>.extraHeader=…`)
    /// or relies on ssh-agent; git must **never** ask for credentials itself.
    /// Two distinct escape hatches have to be closed:
    ///
    /// * **Terminal prompt** — without `GIT_TERMINAL_PROMPT=0` a clone/fetch
    ///   whose credential didn't resolve tries to read a username from
    ///   `/dev/tty`, which for a windowless GUI subprocess fails with the opaque
    ///   `fatal: could not read Username …: Device not configured` (macOS) or
    ///   hangs. The askpass hooks are cleared for the same reason.
    /// * **Credential helper** — a helper configured in the user's git config
    ///   (`credential.helper = osxkeychain` / `manager` / `libsecret`) fires on
    ///   *every* operation, popping an OS keychain prompt each time (the "asks
    ///   for the password on every operation" symptom) or injecting a duplicate
    ///   `Authorization` header. We neutralise it via the `GIT_CONFIG_*` env
    ///   trio — an empty `credential.helper` applied last resets the helper
    ///   chain for this invocation only, without touching the user's config or
    ///   threading `-c` args through every call site.
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.no_window();
        c.env("GIT_TERMINAL_PROMPT", "0");
        c.env("GIT_ASKPASS", "");
        c.env("SSH_ASKPASS", "");
        c.env("GCM_INTERACTIVE", "never");
        c.env("GIT_CONFIG_COUNT", "1");
        c.env("GIT_CONFIG_KEY_0", "credential.helper");
        c.env("GIT_CONFIG_VALUE_0", "");
        c
    }
}
