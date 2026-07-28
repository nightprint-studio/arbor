//! `config` — the typed **product** picus configuration
//! (`arbor/profiles/<active>/picus/config.toml`, per-profile) owned
//! **out-of-process** by `picus-be`.
//!
//! Holds the studio's persisted preferences: the encoding fallbacks, the write
//! guards, the emission defaults and the query row limit. The sections mirror the
//! settings modal one-for-one, so a new setting has an obvious home.
//!
//! ## What deliberately is NOT here
//!
//! A script project's own settings — its declared encoding, its line ending, its
//! version table — belong to the **project**, not to this machine's profile: a
//! colleague opening the same repository must inherit them. They land in the
//! project's config when the script half of the backend does; putting them here
//! would make the same repository behave differently per user, which is exactly the
//! class of surprise Picus exists to remove.
//!
//! Like `bennu-core`'s config, the path is **not** pushed by the shell: picus-be
//! resolves [`picus_config_path`](arbor_core::prelude::picus_config_path) itself,
//! since `init_active_profile()` ran in `main` before any handler is served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`PicusConfig::default`] so operational reads never break. The
//! `get/set_picus_config` handlers stay in picus-be and call back into [`load`] /
//! [`save`] here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a generated block lands in a destination file.
///
/// Defined in `picus-project`, which owns the project file that can override it
/// per folder role, and re-exported here because this file holds the *user's*
/// default for the same setting. One type, so the two tiers cannot disagree about
/// what `"end-of-file"` means.
pub use picus_project::prelude::InsertionRule;

/// Persisted picus settings (product, per-profile `…/picus/config.toml`).
///
/// Field order matters for TOML serialization: every scalar field must be declared
/// before the nested-table fields, or `toml` fails with "values must be emitted
/// before tables". This struct is all tables, so a scalar added later goes on top.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PicusConfig {
    /// How undecidable file encodings are resolved.
    pub encoding: PicusEncodingConfig,
    /// The guards that stand between a generated block and the disk.
    pub writing: PicusWritingConfig,
    /// Defaults applied while emitting SQL.
    pub generation: PicusGenerationConfig,
    /// Result-grid fetch behaviour.
    pub queries: PicusQueryConfig,
}

/// Encoding fallbacks. Detection itself is per file (BOM → valid UTF-8 with a
/// multibyte sequence → ambiguous); these settle the ambiguous case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PicusEncodingConfig {
    /// Fallback for files the heuristics cannot decide (pure ASCII, no BOM).
    /// Legacy script repositories are overwhelmingly `windows-1252`.
    pub default: String,
    /// Treat a pure-ASCII file as neutral and inherit the folder's dominant
    /// encoding instead of stamping [`default`](Self::default) on it.
    pub inherit_ascii: bool,
}

impl Default for PicusEncodingConfig {
    fn default() -> Self {
        Self { default: "windows-1252".to_string(), inherit_ascii: true }
    }
}

/// Write guards. Both default to on: Picus rewrites files a team depends on, and
/// turning either off should be a deliberate act, not a default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PicusWritingConfig {
    /// Show the diff and ask before touching disk.
    pub confirm_before_write: bool,
    /// Copy every file to `.arbor/backup/<timestamp>/` before rewriting it.
    pub backup_before_write: bool,
}

impl Default for PicusWritingConfig {
    fn default() -> Self {
        Self { confirm_before_write: true, backup_before_write: true }
    }
}

/// Emission defaults.
///
/// The two insertion rules are stored **as strings on purpose** — see
/// [`InsertionRule`]. Read them through [`insertion_rule_init`](Self::insertion_rule_init)
/// / [`insertion_rule_update`](Self::insertion_rule_update), which give the typed
/// value with a defined fallback.
///
/// These are *this user's* defaults. A repository that states its own rule in
/// `.arbor/picus/project.toml` outranks them, because where a block lands is
/// visible in every colleague's diff and therefore belongs to the repository.
/// The resolution order is: project file → here → [`InsertionRule::default_for`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PicusGenerationConfig {
    /// Where a generated block lands in an **initialisation** script. Defaults to
    /// grouping with the statements on the same table, which is how init scripts
    /// are read.
    pub insertion_rule_init: String,
    /// Where a generated block lands in an **update** script. Defaults to the end
    /// of the file: an update script is a chronological log, and appending is the
    /// rule a reader can predict without opening the file.
    pub insertion_rule_update: String,
    /// Lowercase identifiers when emitting PostgreSQL. The Oracle side is never
    /// affected — its folder's dialect decides, not this flag.
    pub lowercase_postgres: bool,
}

impl Default for PicusGenerationConfig {
    fn default() -> Self {
        Self {
            insertion_rule_init:   InsertionRule::AfterLastOnTable.as_wire().to_string(),
            insertion_rule_update: InsertionRule::EndOfFile.as_wire().to_string(),
            lowercase_postgres:    true,
        }
    }
}

impl PicusGenerationConfig {
    /// The typed insertion rule for initialisation scripts (default on an unknown
    /// value).
    pub fn insertion_rule_init(&self) -> InsertionRule {
        InsertionRule::from_wire(&self.insertion_rule_init)
            .unwrap_or(InsertionRule::AfterLastOnTable)
    }

    /// The typed insertion rule for update scripts (default on an unknown value).
    pub fn insertion_rule_update(&self) -> InsertionRule {
        InsertionRule::from_wire(&self.insertion_rule_update).unwrap_or(InsertionRule::EndOfFile)
    }
}

/// Result-grid fetch behaviour.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PicusQueryConfig {
    /// Rows fetched per page. Paging bounds what crosses the wire; the grid's
    /// virtualisation bounds what is drawn — which is why the ceiling is generous.
    /// Clamp with [`row_limit`](Self::row_limit) rather than trusting the field.
    pub row_limit: u32,
}

impl Default for PicusQueryConfig {
    fn default() -> Self {
        Self { row_limit: DEFAULT_ROW_LIMIT }
    }
}

impl PicusQueryConfig {
    /// The row limit, clamped into [`ROW_LIMIT_RANGE`]. A hand-edited `0` in the
    /// TOML would otherwise mean "fetch nothing" and look like a broken product.
    pub fn row_limit(&self) -> u32 {
        self.row_limit.clamp(*ROW_LIMIT_RANGE.start(), *ROW_LIMIT_RANGE.end())
    }
}

/// Default rows per page — the same default the settings modal shows.
pub const DEFAULT_ROW_LIMIT: u32 = 500;

/// Accepted range for the query row limit.
pub const ROW_LIMIT_RANGE: std::ops::RangeInclusive<u32> = 1..=100_000;

// ── Persistence ────────────────────────────────────────────────────────────────

/// picus's own config file: `arbor/profiles/<active>/picus/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::picus_config_path("config.toml")
}

/// Read the picus config. A missing / unparseable file yields defaults, never an
/// error — studio preferences are non-critical and self-heal to defaults.
pub fn load() -> PicusConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<PicusConfig>(&text) {
            return cfg;
        }
    }
    PicusConfig::default()
}

/// Persist the picus config to its own file (pretty TOML), creating the dir if
/// needed.
pub fn save(cfg: &PicusConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let cfg = PicusConfig::default();
        let text = toml::to_string_pretty(&cfg).expect("serialize");
        let back: PicusConfig = toml::from_str(&text).expect("deserialize");

        assert_eq!(back.encoding.default, "windows-1252");
        assert!(back.encoding.inherit_ascii);
        assert!(back.writing.confirm_before_write);
        assert!(back.writing.backup_before_write);
        assert!(back.generation.lowercase_postgres);
        assert_eq!(back.queries.row_limit(), DEFAULT_ROW_LIMIT);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        // A hand-edited config that mentions one setting must not blank the rest.
        let cfg: PicusConfig =
            toml::from_str("[writing]\nconfirm_before_write = false\n").expect("deserialize");

        assert!(!cfg.writing.confirm_before_write);
        assert!(cfg.writing.backup_before_write, "untouched sibling keeps its default");
        assert_eq!(cfg.encoding.default, "windows-1252", "untouched section keeps its defaults");
    }

    #[test]
    fn insertion_rules_round_trip_and_unknown_falls_back_per_role() {
        for rule in
            [InsertionRule::EndOfFile, InsertionRule::AfterLastOnTable, InsertionRule::BeforeFinalCommit]
        {
            assert_eq!(InsertionRule::from_wire(rule.as_wire()), Some(rule));
        }
        assert_eq!(InsertionRule::from_wire("whatever"), None);

        // An unknown value degrades to the role's default — and, crucially, parses:
        // the rest of the user's settings survive.
        let cfg: PicusConfig = toml::from_str(
            "[generation]\ninsertion_rule_init = \"nonsense\"\ninsertion_rule_update = \"nonsense\"\nlowercase_postgres = false\n",
        )
        .expect("an unknown rule must not fail the parse");

        assert_eq!(cfg.generation.insertion_rule_init(), InsertionRule::AfterLastOnTable);
        assert_eq!(cfg.generation.insertion_rule_update(), InsertionRule::EndOfFile);
        assert!(!cfg.generation.lowercase_postgres, "the sibling setting still applied");
    }

    #[test]
    fn row_limit_is_clamped_not_trusted() {
        let mut q = PicusQueryConfig { row_limit: 0 };
        assert_eq!(q.row_limit(), 1, "0 would mean 'fetch nothing'");

        q.row_limit = 10_000_000;
        assert_eq!(q.row_limit(), *ROW_LIMIT_RANGE.end());

        q.row_limit = 1_000;
        assert_eq!(q.row_limit(), 1_000, "a value in range passes through");
    }
}
