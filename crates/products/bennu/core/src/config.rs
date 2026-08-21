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
    /// Which debug adapter to drive a native (Rust) debug session with: `codelldb`, `lldb-dap` or
    /// `gdb`. Empty = whichever is installed, in that order of preference.
    ///
    /// Worth pinning because the three are not interchangeable in the way that matters: only CodeLLDB
    /// renders Rust's own types, so a `Vec<T>` is its elements under one and a pointer and a length
    /// under the others. A pinned adapter that is missing is reported rather than silently replaced.
    #[serde(default)]
    pub debug_adapter: String,
    /// An explicit path to that adapter's executable, when it is somewhere the search does not look.
    #[serde(default)]
    pub debug_adapter_path: String,
    /// **Autosave**: write a modified buffer to disk automatically — after a short idle, on switching
    /// tabs, and when the window loses focus. `true` by default (IntelliJ-style); turn it off to save
    /// only explicitly (Ctrl+S).
    pub autosave: bool,
    /// **Fold runs of library frames** in the debugger's call stack: consecutive frames outside this
    /// project collapse into one expandable row. `true` by default — a stop inside a framework is
    /// forty frames of Spring and reflection around the three that are yours, and scrolling past
    /// them to find your own is what the fold exists to remove. Turn it off to see every frame.
    ///
    /// Here rather than per-repo because it is a preference about how *you* read a stack, not
    /// something about a project.
    pub collapse_library_frames: bool,
    /// **Packages a debugger step passes straight through**, as JDWP class-name patterns
    /// (`java.*`, `org.springframework.*`). Empty means "use the defaults" — see
    /// `bennu_be`'s `DEFAULT_STEP_EXCLUDES`, which is the JDK plus the machinery that sits
    /// between a caller and an injected bean.
    ///
    /// A pattern may carry a `*` at **one end only** — that is all the protocol accepts, and one
    /// with a star in the middle makes the VM refuse the whole step request, which reads as
    /// stepping having silently stopped working. Invalid entries are dropped rather than sent.
    ///
    /// Editable because the defaults are a judgement about whose code you are debugging: they
    /// make stepping into your own service usable and stepping into Spring impossible, and
    /// which of those you want is not something a default can know.
    pub step_excludes: Vec<String>,
    /// **Search the dependency jars too** in the Go-to navigator: two extra categories offering
    /// the classes and the files that are on the classpath but nowhere in the project tree —
    /// the `struts-default.xml` that declares an interceptor stack, a schema, a framework
    /// annotation whose package you are trying to remember.
    ///
    /// Off by default, and a setting rather than always-on, because it changes what the box
    /// *is*: with it on, the answer to a two-letter query includes a hundred thousand things
    /// you did not write. On a legacy project it is the difference between reading the
    /// framework and guessing at it; on a small one it is noise.
    ///
    /// It costs nothing until you type — those categories are searched in the backend, not
    /// shipped — but the first search after opening a project pays for listing the jars.
    pub search_dependencies: bool,
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
    /// **Indexing CPU budget**: the maximum worker threads the background sweeps may use — the
    /// initial index build, the reference walk behind find-usages, the encoding scan.
    ///
    /// `1` by default, which means **serial**. The previous behaviour was `available_parallelism −
    /// 2`, chosen to leave the foreground room and in practice leaving none: six saturated cores on
    /// an eight-core machine is felt as a stall in everything, the editor included, for as long as
    /// a large project takes to parse. A background job that makes the machine unusable has not
    /// earned its speed.
    ///
    /// Raise it when indexing feels slow and the machine has room — `0` restores the automatic
    /// budget. Distinct from [`validation_threads`](Self::validation_threads), which caps the
    /// separate whole-project validation sweep.
    pub index_threads: usize,
    /// **Local history**: keep a private record of what every project file used to be, so a
    /// save, a refactor or a delete can be undone long after the editor's own undo stack has
    /// moved on. `true` by default. Stored in Arbor's data directory, never inside the
    /// project — a history folder inside a repository is a folder that gets committed.
    pub local_history: bool,
    /// How many days of local history to keep. Labelled revisions, and each file's newest
    /// one, are kept regardless — a label is a promise, and a file whose only revision aged
    /// out would quietly stop having a history exactly when it is the last copy.
    pub local_history_days: u32,
    /// Ceiling on one project's local history, in megabytes. Over it, the oldest revisions go.
    pub local_history_max_mb: u64,
    /// Files bigger than this (megabytes) are not recorded. One 40 MB binary would spend the
    /// whole budget on a single revision that no diff can show anyway.
    pub local_history_max_file_mb: u64,
    /// Extra JDK install directories to search, **before** `JAVA_HOME` and each platform's standard
    /// install roots (see `bennu_classpath`'s `jdk_install_roots`). For a JDK installed somewhere
    /// non-standard — a portable SDK, an unpacked tarball — so the index can still resolve the
    /// standard library. Each is a JDK home (the dir holding `release`), or, on macOS, the `.jdk`
    /// bundle wrapping one.
    pub jdk_paths: Vec<String>,
    /// Per-project JDK override, keyed by absolute project-root path → Java version
    /// string (e.g. `"17"`). Present entries win over the pom-detected JDK.
    pub jdk_overrides: BTreeMap<String, String>,
    /// Per-project (or per-file) encoding override, keyed by absolute path → encoding
    /// label (e.g. `"Cp1252"`). Present entries win over the pom-declared encoding.
    pub encoding_overrides: BTreeMap<String, String>,
    /// Which `application*.yml` / `application*.properties` a project's Spring
    /// `${placeholder}`s resolve against, keyed by absolute (forward-slashed) project root
    /// → absolute file path.
    ///
    /// A real project has several — `application.yml`, `application-dev.yml`, one per
    /// module — and which one is *running* is a launch argument, not something the sources
    /// reveal. So the editor doesn't guess: an absent entry resolves against the
    /// profile-less files (what Spring always loads), and an entry pins the user's choice.
    /// A stale path (the file was deleted or renamed) is ignored rather than breaking
    /// resolution.
    pub spring_property_files: BTreeMap<String, String>,
    /// Explicit **JSP → Struts action** binding, keyed by absolute (forward-slashed) JSP path →
    /// action qualified-name. For a view-only JSP (OGNL, no `<form>`) that maps to several actions
    /// — or none the reverse-lookup can see — the user pins which action the page's properties are
    /// checked/navigated against. Empty (the common case) → the binding is auto-resolved from the
    /// page's forms + the single reverse-lookup candidate.
    pub jsp_action_bindings: BTreeMap<String, String>,
    /// Which **dependencies contribute their Spring beans** to the Library beans view.
    ///
    /// Empty by default, and empty means no jar is ever opened — this reads a project's
    /// third-party code, which has to be asked for. The four axes are how a coordinate gets
    /// matched in practice: one artifact, a whole group, everything an organisation
    /// publishes (`com.acme.` — the trailing dot matters, since `com.acme` also admits
    /// `com.acmegroup`), or a naming convention (`acme-starter-`).
    ///
    /// The intended entries are your **own** shared modules, whose beans are plain
    /// `@Service` / `@Configuration` and therefore simply true. Allowlisting Spring Boot's
    /// own starters is allowed, but shows conditional beans — declarations Spring may or may
    /// not act on — which is why each one carries the conditions gating it and why none of
    /// them take part in injection resolution or in any diagnostic.
    ///
    /// A table, so it lives at the end: TOML requires every value before any table (see the
    /// note on this struct), and a scalar declared after this one would be read back as a
    /// key of it.
    pub library_beans: LibraryBeansConfig,
    /// **Language servers** — which ones may run, where their binaries are, and any the user
    /// added themselves. See [`LspConfig`].
    ///
    /// A table, so it must stay beside `library_beans` at the end of the struct.
    pub lsp: LspConfig,
    /// **Cargo / crates.io** — the one part of Bennu that reaches the network on its own. See
    /// [`CargoConfig`].
    ///
    /// A table: it stays at the end with the others.
    pub cargo: CargoConfig,
    /// **First-run tour** — whether the user has been through Bennu's, and at which schema
    /// version. See [`OnboardingConfig`].
    ///
    /// Bennu's own, not the shell's: Corvus keeps the same two fields in `corvus/config.toml`,
    /// and finishing one tour is no reason to stop introducing the other product.
    ///
    /// A table: it stays at the end with the others.
    #[serde(default)]
    pub onboarding: OnboardingConfig,
}

/// Whether the user has been through Bennu's welcome tour.
///
/// `version` is a schema knob rather than a build number: the frontend re-opens the tour when
/// its own `CURRENT_ONBOARDING_VERSION` exceeds the stored one, which is how a release that
/// adds a genuinely new step gets to show it to somebody who has already been through the old
/// ones. `0` means never seen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OnboardingConfig {
    /// Finished or skipped at least once.
    pub completed: bool,
    /// The schema version that happened at.
    pub version: u32,
}

/// Cargo settings — specifically, Bennu's use of the crates.io index.
///
/// Everything else about a Rust project is read off the machine (the manifests, `Cargo.lock`, the
/// unpacked sources in `$CARGO_HOME`). Two questions cannot be: "is there a newer version of this
/// crate" and "what versions can I pick when adding one". Both are answered from
/// `index.crates.io`, and that deserves a switch rather than being an unannounced fact about the
/// editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CargoConfig {
    /// Whether Bennu may query the crates.io index.
    ///
    /// `true` by default: a Rust editor that cannot say a dependency is three minor versions behind
    /// is missing something a Rust developer expects, and the traffic is one small text file per
    /// crate per [`Self::index_ttl_hours`]. Off makes Bennu entirely local again — version hints
    /// disappear, and adding a dependency still works, it just cannot offer the version list.
    pub crates_io: bool,
    /// How long a cached version list stays fresh, in hours.
    ///
    /// A day by default, which is the right order of magnitude for the question being asked: crates
    /// publish weekly at most, and "your dependency is behind" does not become more true by being
    /// checked hourly. `0` is read as the default rather than as "always refetch" — a TTL of zero
    /// would mean a request per crate per manifest open, which is exactly what the cache exists to
    /// prevent.
    pub index_ttl_hours: u32,
}

impl Default for CargoConfig {
    fn default() -> Self {
        Self { crates_io: true, index_ttl_hours: 24 }
    }
}

/// Language-server settings.
///
/// Bennu's Java intelligence is its own engine; every other language is served by an external
/// language server. The built-in catalogue (`bennu_lsp`'s `BUILTIN_SERVERS`) knows how to run
/// a handful of them, and [`servers`](Self::servers) is how a language nobody anticipated is
/// added without a new release.
///
/// Field order matters for TOML: the scalar and inline-array values are declared before the
/// map and the array-of-tables.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LspConfig {
    /// Master switch. `true` by default: a server is only ever started for a project whose
    /// root carries the matching manifest *and* whose binary is installed, so "on" costs
    /// nothing on a machine with nothing installed.
    pub enabled: bool,
    /// What rust-analyzer runs to produce **real** diagnostics on save: `check` or `clippy`.
    ///
    /// `check` by default, because it is what `cargo build` would have told you and it is the
    /// faster of the two. `clippy` is a superset — every `cargo check` error plus several hundred
    /// lints — and costs a slower build after each save, which on a large workspace is the
    /// difference between diagnostics landing in two seconds and in ten.
    ///
    /// Server-specific in an otherwise generic struct, which is deliberate rather than
    /// overlooked: it is the one such knob with a UI behind it, and the alternative — a free-form
    /// per-server JSON blob — cannot be safely edited by a toggle that does not know what else is
    /// in it. If a second one ever appears, a `[lsp.<server-id>]` section is its home, not a
    /// second scalar here.
    pub rust_check_command: String,
    /// Server ids the user turned off (`rust-analyzer`, or a custom server's id).
    ///
    /// A denylist rather than an allowlist so that a server added to the catalogue later
    /// works without the user editing anything — the same reason `step_excludes` is empty by
    /// default.
    pub disabled: Vec<String>,
    /// Explicit executable path per server id, for a binary discovery does not find (or a
    /// specific build the user wants). An absolute path wins over everything; a bare name is
    /// looked up like any command.
    pub server_paths: BTreeMap<String, String>,
    /// **User-defined servers**, for a language the catalogue does not cover. See
    /// [`CustomLspServer`]. An entry whose `id` matches a built-in replaces it, which is how
    /// a server is reconfigured rather than merely re-pointed.
    pub servers: Vec<CustomLspServer>,
    /// How long a language server with **no window showing its project** may sit idle before it is
    /// stopped, in seconds. `0` never stops one.
    ///
    /// Such a session exists because something asked a question about a project nobody has open —
    /// an AI client, in practice. Left alone it would live as long as the backend: rust-analyzer is
    /// most of a gigabyte resident, and the ceiling on how many is the number of projects that can
    /// be asked about, which is not a ceiling. This is the only thing that reclaims one.
    ///
    /// Ten minutes by default — long enough that a session of related questions never pays a cold
    /// restart, short enough that a machine does not accumulate them over an afternoon. A server a
    /// window opened is **never** stopped by this, whatever it says: something is on screen, and
    /// taking it away costs a rebuild the moment it is looked at.
    pub background_idle_timeout_secs: u64,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rust_check_command: "check".to_string(),
            disabled: Vec::new(),
            server_paths: BTreeMap::new(),
            servers: Vec::new(),
            background_idle_timeout_secs: 600,
        }
    }
}

/// One language server the user configured by hand.
///
/// The same fields the built-in catalogue carries, because the two are interchangeable by
/// design: anything Bennu can do for Rust it can do for a language whose server is described
/// here, with no code change.
///
/// All fields are scalars or inline arrays, so this serializes as an
/// `[[lsp.servers]]` array-of-tables.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CustomLspServer {
    /// Stable id — the key for a path override or a disable. Required; an entry without one
    /// is ignored.
    pub id: String,
    /// Display name. Falls back to `id` when empty.
    pub name: String,
    /// The LSP `languageId` to send in `didOpen` (`"zig"`, `"ruby"`). Falls back to `id`.
    ///
    /// Worth setting correctly: some servers branch on it, and it is also Bennu's own key for
    /// "which server owns this language".
    pub language: String,
    /// The executable. Looked up the same way a catalogue command is (an absolute path is
    /// used as-is).
    pub command: String,
    /// Arguments. Several servers default to a socket and need `--stdio` here.
    pub args: Vec<String>,
    /// File extensions it serves, without dots (`["zig"]`). Required — an entry serving no
    /// extension can never be selected.
    pub extensions: Vec<String>,
    /// Files whose presence marks a workspace root (`["build.zig"]`). Required, and the real
    /// gate: without a marker above the file there is no workspace to open, so nothing starts
    /// — which is what keeps a stray `.py` in a Java repo from spawning a Python server.
    pub root_markers: Vec<String>,
    /// `initializationOptions` as a **JSON string** (`'{"checkOnSave":true}'`).
    ///
    /// A string rather than a nested table because these are arbitrary server-defined JSON —
    /// booleans, nested objects, arrays of objects — and TOML cannot express all of it without
    /// a conversion whose edge cases would silently change what the server receives. Invalid
    /// JSON is ignored and logged rather than failing the whole config.
    pub initialization_options: String,
}

/// Which dependencies' beans are read. See [`BennuConfig::library_beans`].
///
/// Mirrors `bennu_spring::prelude::LibraryBeanAllowlist`, and deliberately does not reuse
/// it: this is the persisted wire shape, and `bennu-core` has no business depending on a
/// framework crate to describe its own config file. The one conversion lives where the two
/// meet (`bennu-be`), which is also the only place that could get it wrong.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LibraryBeansConfig {
    /// Exact group ids (`com.acme.platform`).
    pub group_id: Vec<String>,
    /// Group-id prefixes (`com.acme.`).
    pub group_id_prefix: Vec<String>,
    /// Exact artifact ids (`shared-security`).
    pub artifact_id: Vec<String>,
    /// Artifact-id prefixes (`acme-starter-`).
    pub artifact_id_prefix: Vec<String>,
}

impl Default for BennuConfig {
    fn default() -> Self {
        Self {
            default_encoding: "UTF-8".to_string(),
            indent_width: 4,
            sql_dialect: "portable".to_string(),
            preferred_build_type: "mvn".to_string(),
            validate_on_open: true,
            // Empty = whichever adapter is installed, preferring the one that renders Rust's own
            // types. Naming one here would pin every user to a debugger they may not have.
            debug_adapter: String::new(),
            debug_adapter_path: String::new(),
            autosave: true,
            collapse_library_frames: true,
            // Empty = the backend's defaults. Writing the list here would freeze a user's
            // config to whatever the defaults were the day they first saved it.
            step_excludes: Vec::new(),
            local_history: true,
            local_history_days: 7,
            local_history_max_mb: 256,
            local_history_max_file_mb: 4,
            search_dependencies: false,
            auto_import: true,
            validation_threads: 0,
            index_threads: 1,
            jdk_paths: Vec::new(),
            jdk_overrides: BTreeMap::new(),
            encoding_overrides: BTreeMap::new(),
            spring_property_files: BTreeMap::new(),
            jsp_action_bindings: BTreeMap::new(),
            library_beans: LibraryBeansConfig::default(),
            lsp: LspConfig::default(),
            cargo: CargoConfig::default(),
            onboarding: OnboardingConfig::default(),
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

    /// A workspace with **no projects** round-trips.
    ///
    /// This is the shape a freshly-created workspace has, and it is the shape the release
    /// data-loss report was about: workspaces appeared, projects did not. An empty
    /// array-of-tables is also the case a pretty TOML serializer is most likely to emit
    /// differently, so it is pinned rather than assumed.
    #[test]
    fn an_empty_workspace_round_trips() {
        let store = BennuWorkspaces {
            active_id: "w1".to_string(),
            workspaces: vec![ws("w1", "Scratch", &[])],
        };
        let text = toml::to_string_pretty(&store).expect("serializes");
        let back = parse_workspaces(&text);
        assert_eq!(back.active_id, "w1");
        assert_eq!(back.workspaces.len(), 1, "an empty workspace is still a workspace");
        assert_eq!(back.workspaces[0].name, "Scratch");
        assert!(back.workspaces[0].projects.is_empty());
    }

    /// The legacy migration must fire **only** on a legacy file. A real store whose workspaces
    /// happen to hold no projects is not one — reading it as legacy would drop every workspace in
    /// it, which is the same data loss by a different route.
    #[test]
    fn a_real_store_is_never_read_as_a_legacy_file() {
        let text = toml::to_string_pretty(&BennuWorkspaces {
            active_id: "w1".to_string(),
            workspaces: vec![ws("w1", "A", &[]), ws("w2", "B", &[])],
        })
        .expect("serializes");
        let back = parse_workspaces(&text);
        assert_eq!(back.workspaces.len(), 2);
    }

    /// A file written by a **newer** Bennu still loads. Every persisted struct here is
    /// `#[serde(default)]`, and this is what that buys: an added field does not turn one upgrade
    /// into an erased session, which without the default would be a parse error, then the legacy
    /// retry, then an empty store written back over the file.
    #[test]
    fn an_unknown_field_does_not_discard_the_file() {
        let text = "\
active_id = \"w1\"
future_flag = true

[[workspaces]]
id = \"w1\"
name = \"A\"
color_idx = 2
active_project = \"/p\"
theme = \"neon\"

[[workspaces.projects]]
root = \"/p\"
open_files = [\"/p/A.java\"]
active_file = \"/p/A.java\"
scroll = 42
";
        let back = parse_workspaces(text);
        assert_eq!(back.active_id, "w1");
        assert_eq!(back.workspaces.len(), 1);
        assert_eq!(back.workspaces[0].projects.len(), 1, "the session survived the unknown keys");
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
