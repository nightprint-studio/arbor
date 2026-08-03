//! `config` — the typed **product** bennu configuration
//! (`arbor/profiles/<active>/bennu/config.toml`, per-profile) owned
//! **out-of-process** by `bennu-be`.
//!
//! Holds the Java-editor's persisted defaults + the IntelliJ-style *overrides* the
//! project model consults: a per-project JDK override (when the pom can't be trusted
//! / a different JDK is wanted) and a per-project / per-file encoding override (the
//! footer-style "reload in encoding X"). The auto-detected values live in the
//! project model; these are only the user's explicit overrides + editor defaults.
//!
//! Like `tyto-core`'s config, the path is **not** pushed by the shell: bennu-be
//! resolves [`bennu_config_path`](arbor_core::prelude::bennu_config_path) itself,
//! since `init_active_profile()` ran in `main` before any handler is served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`BennuConfig::default`] so operational reads never break. The
//! `get/set_bennu_config` handlers stay in bennu-be and call back into [`load`] /
//! [`save`] here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted bennu settings (product, per-profile `…/bennu/config.toml`).
///
/// Field order matters for TOML serialization: every scalar field is declared
/// before the map/table fields (`jdk_overrides` / `encoding_overrides`), or `toml`
/// fails with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BennuConfig {
    /// Default text encoding to *fall back to* when a project declares none and no
    /// override applies. `"UTF-8"` by default (the declared pom encoding always wins
    /// over this — see `bennu-project`'s encoding detection).
    pub default_encoding: String,
    /// Editor indentation width in spaces (the whitespace normalizer / display).
    pub indent_width: u32,
    /// Which SQL dialect `.sql` buffers are **highlighted** as: `"oracle"`,
    /// `"postgres"`, or `"portable"` (the default). A setting and not a detection,
    /// because a `.sql` file in a Java project's resources carries nothing that says
    /// which engine it targets, and guessing wrong is visible: Oracle's `q'[…]'` and
    /// PostgreSQL's `$$ … $$` are each a broken string under the other's rules.
    /// `"portable"` uses the rules valid on both — the honest answer for a file
    /// nobody has classified. Empty is treated as `"portable"`.
    pub sql_dialect: String,
    /// The build the split-button runs by default (and on Ctrl+F9): `"mvn"` (Maven compile) or
    /// `"validate"` (whole-project validation without compiling). Empty is treated as `"mvn"`.
    pub preferred_build_type: String,
    /// Whether to warm up the whole-project **validation cache** in the background right after a
    /// project finishes indexing, so the first explicit "Validate (no compile)" is instant (and the
    /// resolved data it computes is ready for navigation features). `true` by default; turn it off
    /// to avoid the one-shot background CPU on every open. The `#[serde(default)]` container fills a
    /// missing key from this struct's `Default` (→ `true`), so existing config files opt in.
    pub validate_on_open: bool,
    /// **Autosave**: write a modified buffer to disk automatically — after a short idle, on switching
    /// tabs, and when the window loses focus. `true` by default (IntelliJ-style); turn it off to save
    /// only explicitly (Ctrl+S).
    pub autosave: bool,
    /// **Auto-import on completion**: when accepting a type-name completion whose simple name resolves
    /// to a SINGLE class, add its `import` line automatically. `true` by default; off inserts just the
    /// name (import it later with Alt+Enter).
    pub auto_import: bool,
    /// **Validation CPU budget**: the maximum worker threads the whole-project validation sweep
    /// (the background warm-up + the explicit "Validate — no compile") may use. `0` = auto (leave
    /// roughly half the cores free for the UI / go-to / completion); set a small number (e.g. `1` for
    /// single-threaded) so a big project's validation can't peg every core and freeze the editor.
    /// Doesn't affect the one-shot initial index build. `#[serde(default)]` fills a missing key with
    /// `0`, so existing config files get the auto behaviour.
    pub validation_threads: usize,
    /// Extra JDK install directories to search, on top of `JAVA_HOME` +
    /// `C:/Program Files/Java/*`. For a JDK installed somewhere non-standard (a portable
    /// SDK, an IDE-bundled JDK, `/usr/lib/jvm/…`), so the index can still resolve the
    /// standard library. Each is a JDK home (the dir holding `release` / `bin/java`).
    pub jdk_paths: Vec<String>,
    /// Per-project JDK override, keyed by absolute project-root path → Java version
    /// string (e.g. `"17"`). Present entries win over the pom-detected JDK.
    pub jdk_overrides: BTreeMap<String, String>,
    /// Per-project (or per-file) encoding override, keyed by absolute path → encoding
    /// label (e.g. `"Cp1252"`). Present entries win over the pom-declared encoding.
    pub encoding_overrides: BTreeMap<String, String>,
    /// Explicit **JSP → Struts action** binding, keyed by absolute (forward-slashed) JSP path →
    /// action qualified-name. For a view-only JSP (OGNL, no `<form>`) that maps to several actions
    /// — or none the reverse-lookup can see — the user pins which action the page's properties are
    /// checked/navigated against. Empty (the common case) → the binding is auto-resolved from the
    /// page's forms + the single reverse-lookup candidate.
    pub jsp_action_bindings: BTreeMap<String, String>,
}

impl Default for BennuConfig {
    fn default() -> Self {
        Self {
            default_encoding: "UTF-8".to_string(),
            indent_width: 4,
            sql_dialect: "portable".to_string(),
            preferred_build_type: "mvn".to_string(),
            validate_on_open: true,
            autosave: true,
            auto_import: true,
            validation_threads: 0,
            jdk_paths: Vec::new(),
            jdk_overrides: BTreeMap::new(),
            encoding_overrides: BTreeMap::new(),
            jsp_action_bindings: BTreeMap::new(),
        }
    }
}

/// One project's editor **session** inside a workspace — its open tabs (which may include files
/// opened from OTHER workspace projects; the FE flags those as foreign) + the active tab. Nested
/// as an array-of-tables under [`BennuWorkspace`], so its fields are all scalars/inline-arrays.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProjectSession {
    /// Absolute (forward-slashed) project root — the session key.
    pub root: String,
    /// The open editor tabs (file paths) in tab order, at last change.
    pub open_files: Vec<String>,
    /// The active tab (one of `open_files`), or empty.
    pub active_file: String,
}

/// One named **workspace** — an ordered set of Java projects, each with its own editor session,
/// so switching workspace reopens a whole different set of projects where the user left off. The
/// same project may belong to several workspaces (each keeps its own tabs). Mirrors Corvus's
/// `WorkspaceDef` (id / name / color) minus the git-specific parts (groups, repo registry).
///
/// Field order matters for TOML: the scalar fields precede the array-of-tables (`projects`), or
/// `toml` fails with "values must be emitted before tables".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BennuWorkspace {
    /// Stable id (FE-generated uuid). Empty only in a legacy single-workspace file (migrated).
    pub id: String,
    /// Display name (e.g. "Backend legacy"). '' for the implicit default workspace.
    pub name: String,
    /// Palette index (0..11) for the workspace monogram — mirrors Corvus `color_idx`.
    pub color_idx: u8,
    /// Root of the active project (one of `projects[].root`), or '' when empty.
    pub active_project: String,
    /// The member projects + their sessions, in switch order.
    pub projects: Vec<ProjectSession>,
}

/// The persisted workspace store (`arbor/profiles/<active>/bennu/workspace.toml`) — every named
/// workspace plus which one is active. Kept in its own file (not `config.toml`): volatile session
/// state that churns on every tab open/close, distinct from the stable editor settings.
///
/// Field order matters for TOML: the scalar `active_id` precedes the array-of-tables `workspaces`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BennuWorkspaces {
    /// Id of the active workspace (one of `workspaces[].id`), or '' when there are none.
    pub active_id: String,
    /// Every workspace, in display order.
    pub workspaces: Vec<BennuWorkspace>,
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// bennu's own config file: `arbor/profiles/<active>/bennu/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("config.toml")
}

/// Read the bennu config. A missing / unparseable file yields defaults, never an
/// error — editor settings are non-critical and self-heal to defaults.
pub fn load() -> BennuConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<BennuConfig>(&text) {
            return cfg;
        }
    }
    BennuConfig::default()
}

/// Persist the bennu config to its own file (pretty TOML), creating the dir if
/// needed.
pub fn save(cfg: &BennuConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// bennu's workspace-session file: `arbor/profiles/<active>/bennu/workspace.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn workspace_path() -> PathBuf {
    arbor_core::prelude::bennu_config_path("workspace.toml")
}

/// Read the workspace store. A missing / unparseable file yields an empty store, never an error
/// (a corrupt session must never block the window from opening).
///
/// **Migration**: a file written before named workspaces existed is a bare [`BennuWorkspace`]
/// (top-level `name` / `active_project` / `projects`). When the new [`BennuWorkspaces`] parse
/// yields no workspaces, we retry as the legacy shape and wrap the single workspace, so the last
/// session is preserved across the upgrade instead of silently dropped.
pub fn load_workspaces() -> BennuWorkspaces {
    match std::fs::read_to_string(workspace_path()) {
        Ok(text) => parse_workspaces(&text),
        Err(_) => BennuWorkspaces::default(),
    }
}

/// Pure parse of a `workspace.toml` body into a [`BennuWorkspaces`], applying the legacy
/// single-workspace migration. Split from [`load_workspaces`] so the parse + migration is unit
/// testable without touching the filesystem.
fn parse_workspaces(text: &str) -> BennuWorkspaces {
    if let Ok(store) = toml::from_str::<BennuWorkspaces>(text) {
        if !store.workspaces.is_empty() {
            return store;
        }
    }
    // Legacy single-workspace file → wrap it into a default-named workspace.
    if let Ok(mut legacy) = toml::from_str::<BennuWorkspace>(text) {
        if !legacy.projects.is_empty() {
            if legacy.id.is_empty() {
                legacy.id = "default".to_string();
            }
            let active_id = legacy.id.clone();
            return BennuWorkspaces { active_id, workspaces: vec![legacy] };
        }
    }
    BennuWorkspaces::default()
}

/// Persist the workspace store (pretty TOML), creating the dir if needed.
pub fn save_workspaces(store: &BennuWorkspaces) -> Result<(), String> {
    let path = workspace_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: &str, name: &str, roots: &[&str]) -> BennuWorkspace {
        BennuWorkspace {
            id: id.to_string(),
            name: name.to_string(),
            color_idx: 3,
            active_project: roots.first().copied().unwrap_or("").to_string(),
            projects: roots
                .iter()
                .map(|r| ProjectSession {
                    root: (*r).to_string(),
                    open_files: vec![format!("{r}/A.java")],
                    active_file: format!("{r}/A.java"),
                })
                .collect(),
        }
    }

    /// A store with nested projects round-trips through pretty TOML (field order — scalars before
    /// the `projects` / `workspaces` arrays-of-tables — must not trip "values before tables").
    #[test]
    fn workspaces_toml_round_trip() {
        let store = BennuWorkspaces {
            active_id: "w1".to_string(),
            workspaces: vec![ws("w1", "Backend", &["c:/a", "c:/b"]), ws("w2", "Portal", &["c:/a"])],
        };
        let text = toml::to_string_pretty(&store).expect("serialize");
        let back = parse_workspaces(&text);
        assert_eq!(back.active_id, "w1");
        assert_eq!(back.workspaces.len(), 2);
        assert_eq!(back.workspaces[0].name, "Backend");
        assert_eq!(back.workspaces[0].color_idx, 3);
        assert_eq!(back.workspaces[0].projects.len(), 2);
        assert_eq!(back.workspaces[0].projects[1].root, "c:/b");
        assert_eq!(back.workspaces[1].active_project, "c:/a");
    }

    /// The same project may live in more than one workspace — a shared root is not deduped away.
    #[test]
    fn shared_project_across_workspaces() {
        let store = BennuWorkspaces {
            active_id: "w2".to_string(),
            workspaces: vec![ws("w1", "A", &["c:/shared"]), ws("w2", "B", &["c:/shared"])],
        };
        let back = parse_workspaces(&toml::to_string_pretty(&store).unwrap());
        assert_eq!(back.workspaces[0].projects[0].root, "c:/shared");
        assert_eq!(back.workspaces[1].projects[0].root, "c:/shared");
    }

    /// A legacy single-workspace file (no `[[workspaces]]` table, top-level `projects`) migrates
    /// into a one-member store with a synthesized id, instead of being dropped.
    #[test]
    fn legacy_single_workspace_migrates() {
        let legacy = "active_project = \"c:/proj\"\n\
                      [[projects]]\n\
                      root = \"c:/proj\"\n\
                      open_files = [\"c:/proj/Main.java\"]\n\
                      active_file = \"c:/proj/Main.java\"\n";
        let store = parse_workspaces(legacy);
        assert_eq!(store.workspaces.len(), 1);
        assert_eq!(store.workspaces[0].id, "default");
        assert_eq!(store.active_id, "default");
        assert_eq!(store.workspaces[0].projects[0].root, "c:/proj");
    }

    /// An empty / unparseable body yields an empty store (no panic, no error).
    #[test]
    fn empty_body_yields_empty_store() {
        assert!(parse_workspaces("").workspaces.is_empty());
        assert!(parse_workspaces("!!! not toml").workspaces.is_empty());
    }

    /// A config written before `autosave` / `auto_import` existed loads with them ON — the
    /// container `#[serde(default)]` fills a missing field from `BennuConfig::default()` (→ `true`),
    /// so upgrading users opt in rather than silently getting `false`.
    #[test]
    fn config_defaults_fill_missing_toggle_keys() {
        let old = "default_encoding = \"UTF-8\"\nindent_width = 4\n";
        let cfg: BennuConfig = toml::from_str(old).expect("parse");
        assert!(cfg.autosave, "autosave defaults on");
        assert!(cfg.auto_import, "auto_import defaults on");
        // And round-trips.
        let back: BennuConfig = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert!(back.autosave && back.auto_import);
    }
}
