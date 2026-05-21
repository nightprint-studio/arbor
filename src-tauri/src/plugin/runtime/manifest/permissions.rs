//! Permission tiers + the `[permissions]` section of `plugin.toml`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Permission levels — string-typed, ordered so callers can compare with `<`.
// ---------------------------------------------------------------------------

/// Generic three-tier read/write capability used by `fs`, `issues`, `notes`,
/// `toolchain`. Higher variants imply lower ones (`Write >= Read >= None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccessLevel {
    #[default]
    None,
    Read,
    Write,
}

/// Git permission with an extra `HistoryRewrite` tier. `HistoryRewrite >= Write
/// >= Read >= None`. History-rewriting operations (rebase, reset --hard,
/// force-push, amend, filter-branch) are granted separately because they can
/// permanently destroy work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GitLevel {
    #[default]
    None,
    Read,
    Write,
    HistoryRewrite,
}

/// Terminal access. `Any` allows arbitrary commands; `Commands` allows only
/// those whose first token appears in `terminal_scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TerminalLevel {
    #[default]
    None,
    Commands,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    /// Allowlist of hostnames the plugin may contact. Empty = no network.
    #[serde(default)]
    pub network: Vec<String>,

    /// Filesystem access. `Read` allows arbor.fs read ops; `Write` allows both
    /// reads and writes. Path scope is controlled by `fs_scope`.
    #[serde(default)]
    pub fs: AccessLevel,
    /// Optional path scope for `arbor.fs.*`. Empty list (default) = sandboxed
    /// to the active repo's directory. `["*"]` = unrestricted (any path).
    /// Otherwise, the listed absolute paths are allowed in addition to the
    /// active repo.
    #[serde(default)]
    pub fs_scope: Vec<String>,

    /// Git access. `Read` enables arbor.repo.* + arbor.notes.* read ops;
    /// `Write` enables non-destructive mutations (commit, branch, fetch,
    /// push, notes write, clone, stash); `HistoryRewrite` enables rebase,
    /// reset --hard, force-push, amend, filter-branch.
    #[serde(default)]
    pub git: GitLevel,

    /// Terminal access level (none / commands / any).
    #[serde(default)]
    pub terminal: TerminalLevel,
    /// Allowed command prefixes when `terminal = "commands"`.
    #[serde(default)]
    pub terminal_scope: Vec<String>,

    /// Read environment variables (os.getenv).
    /// Accepts:
    ///   - `true`  → all vars readable
    ///   - `false` → os.getenv removed
    ///   - `["PATH", "JAVA_HOME"]` → allowlist; non-listed names return nil
    #[serde(default = "default_env_read")]
    pub env_read: EnvReadPerm,

    /// Issues (Linear / Jira). `Read` → search / get; `Write` → transition / comment.
    #[serde(default)]
    pub issues: AccessLevel,

    /// Git provider host APIs (GitHub PRs / GitLab MRs / CI runs).
    /// `Read` → arbor.mr.list, arbor.ci.runs_for_branch and friends.
    /// `Write` → reserved for future MR/CI mutations (create comment, retrigger, etc.).
    /// Tokens stay in the OS keyring; plugins only see the resolved data.
    #[serde(default)]
    pub provider: AccessLevel,

    /// Toolchain manager. `Read` → list / active / detect / env;
    /// `Write` → add / remove / set_active.
    #[serde(default)]
    pub toolchain: AccessLevel,

    /// Allow arbor.service.export — register callable services for other plugins.
    #[serde(default)]
    pub service_export: bool,
    /// Allow arbor.service.call — invoke services exported by other plugins.
    #[serde(default)]
    pub service_call: bool,
    /// Allow `arbor.settings.read(plugin_name, key)` / `read_project(...)` to
    /// read OTHER plugins' settings (own settings are always readable). Cross-
    /// plugin writes are not exposed: write requires going through
    /// `arbor.service.call`.
    #[serde(default)]
    pub settings_read_others: bool,
}

fn default_env_read() -> EnvReadPerm { EnvReadPerm::All(true) }

// ---------------------------------------------------------------------------
// env_read permission — accepts bool or string allowlist
// ---------------------------------------------------------------------------

/// Controls access to `os.getenv` from plugin Lua.
///
///   * `All(true)`  — all environment variables readable
///   * `All(false)` — `os.getenv` removed entirely
///   * `Allowlist`  — only listed names return a value; others return nil
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvReadPerm {
    All(bool),
    Allowlist(Vec<String>),
}

impl Default for EnvReadPerm {
    fn default() -> Self { EnvReadPerm::All(true) }
}

impl EnvReadPerm {
    /// True when `os.getenv` should be available at all (with or without filtering).
    pub fn any_access(&self) -> bool {
        match self {
            EnvReadPerm::All(b)       => *b,
            EnvReadPerm::Allowlist(v) => !v.is_empty(),
        }
    }

    /// True when *any* var name is readable without filtering.
    pub fn unrestricted(&self) -> bool {
        matches!(self, EnvReadPerm::All(true))
    }

    /// True when this specific var name is allowed.
    pub fn allows(&self, name: &str) -> bool {
        match self {
            EnvReadPerm::All(b)       => *b,
            EnvReadPerm::Allowlist(v) => v.iter().any(|n| n == name),
        }
    }
}
