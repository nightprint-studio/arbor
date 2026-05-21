//! Cross-plugin contribution registry.
//!
//! A *contribution point* is a named extension slot owned by one plugin (the
//! "host"). Other plugins push *contributions* — small data items addressed to
//! the point — via `arbor.ui.contribute(point, item)`. The host reads the
//! merged list at render time.
//!
//! Built on top of the existing service / event bus primitives — does NOT add
//! any new IPC layer. Contributions are pure data; the host plugin (or the
//! frontend, when the point is consumed by built-in UI) interprets the
//! `payload` JSON in whichever way makes sense for that point.
//!
//! Stable contract:
//!   • `point` is a free-form string. Naming convention: `"<owner>:<scope>"`
//!     for built-in points (`"arbor:context-menu"`, `"arbor:command-palette"`),
//!     and `"<plugin-name>:<scope>"` for plugin-owned points
//!     (`"compile-action:tree:node-action"`).
//!   • `item_id` is unique per (plugin_name, point). Re-contributing with the
//!     same id replaces the previous value (idempotent updates).
//!   • Contributions are ordered by `priority` ascending, then by registration
//!     order. Lower priority renders first.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Coalescing window for `arbor://contributions-changed`. Bursts of writes to
/// the same point within this window collapse to a single event.
const COALESCE_WINDOW_MS: u64 = 16;

// ---------------------------------------------------------------------------
// Built-in contribution points
//
// Every UI surface is a well-known contribution point. Plugins push items via
// `arbor.ui.contribute(point, item)` (or one of the sugar APIs that wraps it).
// Frontend consumers read with `list_plugin_contributions(point)`.
// ---------------------------------------------------------------------------

pub mod points {
    // ── Context menus ────────────────────────────────────────────────────────
    // Per-target so the consumer (commit menu, branch menu, file menu, …)
    // subscribes only to its own slot. The `target` is also kept in the
    // payload for back-compat / introspection.
    //
    // Known targets used by built-in UI:
    //   commit | branch | tag | stash | file | remote | submodule
    //   worktree | line | hunk | tab
    //
    // Plugins can also use custom targets — `arbor:context-menu:<custom>` —
    // and consume them from their own UI. Use `context_menu_point(target)` to
    // build the full point name.
    pub const CONTEXT_MENU_PREFIX: &str = "arbor:context-menu";

    #[allow(dead_code)]
    pub const CONTEXT_MENU_COMMIT:    &str = "arbor:context-menu:commit";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_BRANCH:    &str = "arbor:context-menu:branch";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_TAG:       &str = "arbor:context-menu:tag";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_STASH:     &str = "arbor:context-menu:stash";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_FILE:      &str = "arbor:context-menu:file";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_REMOTE:    &str = "arbor:context-menu:remote";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_SUBMODULE: &str = "arbor:context-menu:submodule";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_WORKTREE:  &str = "arbor:context-menu:worktree";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_LINE:      &str = "arbor:context-menu:line";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_HUNK:      &str = "arbor:context-menu:hunk";
    #[allow(dead_code)]
    pub const CONTEXT_MENU_TAB:       &str = "arbor:context-menu:tab";

    /// Build the per-target context-menu point from a `target` string.
    pub fn context_menu_point(target: &str) -> String {
        format!("{}:{}", CONTEXT_MENU_PREFIX, target)
    }

    // ── Other built-in points ────────────────────────────────────────────────
    /// Top-level (hamburger) menu entries.
    pub const MENU:            &str = "arbor:menu";
    /// Sidebar sections (left or right activity bar, top or bottom panel).
    pub const SIDEBAR:         &str = "arbor:sidebar";
    /// Activity bar entries (action / combo / separator) outside sidebars.
    pub const ACTIVITY_BAR:    &str = "arbor:activitybar";
    /// Command palette entries (Ctrl+K).
    pub const COMMAND_PALETTE: &str = "arbor:command-palette";
    /// Keyboard shortcuts.
    pub const KEYBINDING:      &str = "arbor:keybinding";
    /// Plugin-supplied SVG icons keyed by id.
    pub const ICON:            &str = "arbor:icon";
    /// Tree-view sidebar snapshot (one item per sidebar id, replace-by-id).
    pub const TREE_STATE:      &str = "arbor:tree-state";
    /// Plugin panel form-DSL content (one per panel id, replace-by-id).
    pub const PANEL_CONTENT:   &str = "arbor:panel-content";
    // ── New decorator / inline-action points (Phase 1 — future-ready) ────────
    // Frontend consumers may not yet exist for all of these; the points are
    // declared up-front so plugins can start contributing and we can wire
    // consumers incrementally without API breakage.

    /// Status bar — left side. Plugin badge / pill rendered after built-in
    /// indicators (e.g. branch state, change counts).
    /// Payload: `{ label, icon?, action?, tooltip?, color? }`.
    pub const STATUS_BAR_LEFT:  &str = "arbor:status-bar:left";
    /// Status bar — right side. Used for transient indicators (job count, …).
    pub const STATUS_BAR_RIGHT: &str = "arbor:status-bar:right";

    /// Title bar — left side, between the app menu and the tabs.
    /// Payload: `{ label?, icon?, action?, tooltip? }`.
    pub const TITLE_BAR_LEFT:   &str = "arbor:title-bar:left";
    /// Title bar — right side, near the notification bell.
    pub const TITLE_BAR_RIGHT:  &str = "arbor:title-bar:right";

    /// Inline action button on each row of the commit detail panel.
    /// Payload: `{ label, icon?, action }` — fired with the commit oid.
    pub const COMMIT_DETAIL_ACTION: &str = "arbor:commit-detail:action";

    /// Toolbar button next to "Copy" / "Wrap" in the diff viewer.
    /// Payload: `{ label?, icon, action, tooltip? }` — fired with the file path.
    pub const DIFF_TOOLBAR: &str = "arbor:diff-toolbar";

    /// Decorator badge / icon next to a branch in the BranchTree sidebar.
    /// Payload: `{ branch_pattern?, label?, icon?, color?, tooltip? }`.
    /// `branch_pattern` is a glob (`*` matches anything); omit to apply to all.
    pub const BRANCH_DECORATOR: &str = "arbor:branch-decorator";

    /// Decorator badge next to a file in the FileDiffList / FileTree.
    /// Payload: `{ path_pattern?, label?, icon?, color?, tooltip? }`.
    pub const FILE_DECORATOR: &str = "arbor:file-decorator";

    /// Quick-action card on the welcome screen (when no repo is open).
    /// Payload: `{ title, description?, icon?, action }`.
    pub const WELCOME_ACTION: &str = "arbor:welcome-action";

    /// Inline action below the commit message editor in StageArea / CommitForm.
    /// Payload: `{ label, icon?, action, tooltip? }` — fired with the staged
    /// summary; the plugin can validate / decorate / spawn pre-commit checks.
    pub const COMMIT_FORM_ACTION: &str = "arbor:commit-form:action";
}

// ---------------------------------------------------------------------------
// Payload schemas for built-in points
//
// These are the canonical shapes plugins must produce. Parsing failures are
// best-effort — a malformed payload is logged + skipped at the consumer
// boundary, never panics. Frontend type definitions mirror these structs.
// ---------------------------------------------------------------------------

pub mod payloads {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ContextMenuPayload {
        /// `"commit" | "branch" | "file" | "tag" | "stash" | …` — consumer-defined.
        /// Optional because the per-target point name (`arbor:context-menu:commit`)
        /// already encodes the target; sugar APIs duplicate it for introspection.
        #[serde(default)] pub target: Option<String>,
        pub label:  String,
        pub action: String,
        #[serde(default)] pub icon: Option<String>,
        // Plugins may include extra optional flags (`danger`, `separator`, …).
        // Serde ignores unknown fields by default, so they pass through.
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MenuPayload {
        pub label:  String,
        pub action: String,
        #[serde(default)] pub icon: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CommandPayload {
        pub title:                              String,
        #[serde(default)] pub description:      Option<String>,
        #[serde(default)] pub icon:             Option<String>,
        #[serde(default)] pub group:            Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct KeybindingPayload {
        pub key:                                String,
        pub action:                             String,
        #[serde(default)] pub ctrl:             bool,
        #[serde(default)] pub shift:            bool,
        #[serde(default)] pub alt:              bool,
        #[serde(default)] pub description:      String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IconPayload {
        /// Raw SVG markup. Referenced as `"plugin:<plugin-name>:<id>"` in any
        /// `icon` field on other points.
        pub svg: String,
    }

    /// Sidebar registration. `kind = "form"` → body via PANEL_CONTENT;
    /// `kind = "tree"` → body via TREE_STATE.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SidebarPayload {
        /// Action fired when the sidebar's icon is clicked (defaults to
        /// `"panel:open:<id>"` when omitted by the plugin).
        pub action:                                  String,
        pub label:                                   String,
        #[serde(default)] pub icon:                  Option<String>,
        #[serde(default)] pub collapsable:           bool,
        /// `"left" | "right"`. Defaults to `"right"` for the new add_sidebar API.
        #[serde(default = "default_side")]
        pub side:                                    String,
        /// `"top" | "bottom"`. Defaults to `"top"`.
        #[serde(default = "default_position")]
        pub position:                                String,
        #[serde(default)] pub tooltip:               Option<String>,
        /// `"form" | "tree"`. Defaults to `"form"`.
        #[serde(default = "default_kind")]
        pub kind:                                    String,
    }

    fn default_side()     -> String { "right".to_string() }
    fn default_position() -> String { "top".to_string() }
    fn default_kind()     -> String { "form".to_string() }

    /// Activity-bar entry. `kind` discriminates action / combo / separator.
    /// `target` selects the host: `"activity_bar"` (default) or `"repo_actions"`
    /// for combos that should appear in the RepoActions panel above the graph.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ActivityBarPayload {
        /// `"action" | "combo" | "separator"`.
        pub kind:                              String,
        /// `"activity_bar" | "repo_actions"`. Defaults to `"activity_bar"`.
        #[serde(default = "default_target")]
        pub target:                            String,
        // ── action variant ────────────────────────────────────────────────
        #[serde(default)] pub action:          Option<String>,
        #[serde(default)] pub label:           Option<String>,
        #[serde(default)] pub icon:            Option<String>,
        // ── combo variant ─────────────────────────────────────────────────
        #[serde(default)] pub run_action:      Option<String>,
        #[serde(default)] pub select_action:   Option<String>,
        #[serde(default)] pub run_icon:        Option<String>,
        #[serde(default)] pub tooltip:         Option<String>,
        #[serde(default)] pub variant:         Option<String>,
        #[serde(default)] pub options:         Vec<Value>,
    }

    fn default_target() -> String { "activity_bar".to_string() }

    /// Tree-view snapshot for `kind = "tree"` sidebars.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TreeStatePayload {
        #[serde(default)] pub title:   Option<String>,
        #[serde(default)] pub nodes:   Value,
        #[serde(default)] pub version: u64,
    }

    /// Form-DSL content for plugin panels (sidebar `kind = "form"`).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PanelContentPayload {
        #[serde(default)] pub title:   Option<String>,
        #[serde(default)] pub nodes:   Value,
        #[serde(default)] pub actions: Value,
    }

    /// Toolbar-style payload covering status-bar / title-bar / commit-detail
    /// / commit-form / diff-toolbar items. Validation is intentionally loose
    /// — only the field types are enforced. The renderer tolerates missing
    /// labels/icons (it skips the affordance), and the sugar APIs that emit
    /// these payloads (`add_toolbar_action`) already enforce their own
    /// minimum (e.g. `action` for clickable items) at the API boundary.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolbarItemPayload {
        #[serde(default)] pub label:        Option<String>,
        #[serde(default)] pub icon:         Option<String>,
        #[serde(default)] pub action:       Option<String>,
        #[serde(default)] pub tooltip:      Option<String>,
        #[serde(default)] pub color:        Option<String>,
    }

    /// Quick-action card on the welcome screen.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct WelcomeActionPayload {
        pub title:                          String,
        pub action:                         String,
        #[serde(default)] pub description:  Option<String>,
        #[serde(default)] pub icon:         Option<String>,
    }

    /// Decorator badge / icon next to a branch in BranchTree.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BranchDecoratorPayload {
        #[serde(default)] pub branch_pattern: Option<String>,
        #[serde(default)] pub label:          Option<String>,
        #[serde(default)] pub icon:           Option<String>,
        #[serde(default)] pub color:          Option<String>,
        #[serde(default)] pub tooltip:        Option<String>,
    }

    /// Decorator badge next to a file in FileDiffList / FileTree.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FileDecoratorPayload {
        #[serde(default)] pub path_pattern: Option<String>,
        #[serde(default)] pub label:        Option<String>,
        #[serde(default)] pub icon:         Option<String>,
        #[serde(default)] pub color:        Option<String>,
        #[serde(default)] pub tooltip:      Option<String>,
    }

    /// Best-effort parse a contribution payload into a typed shape. Returns
    /// `None` and emits a tracing warning when the JSON doesn't match — the
    /// frontend / consumer should tolerate the missing item.
    #[allow(dead_code)]
    pub fn parse<T: for<'de> Deserialize<'de>>(
        plugin: &str, point: &str, item_id: &str, value: &Value,
    ) -> Option<T> {
        match serde_json::from_value::<T>(value.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(
                    target: "plugin",
                    "[{plugin}] contribution to '{point}' (id='{item_id}') has malformed payload: {e}"
                );
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Schema validation for built-in points (Phase 5)
//
// Built-in contribution points have known payload shapes (see `payloads`
// module). At register time we run the JSON through serde to catch malformed
// payloads BEFORE they enter the registry, so a typo in a Lua call is logged
// once and the bad item is dropped — instead of failing silently at render
// time inside the consumer (which used to swallow the error).
//
// Plugin-defined points (any name not matched here) are NOT validated; their
// schema is whatever the host plugin documents informally. A future
// `arbor.contribution.declare_point(name, schema)` API may add per-point
// validation for those — out of scope for Phase 5.
// ---------------------------------------------------------------------------

/// Validate `payload` against the known schema for `point`. Returns `Ok(())`
/// for plugin-defined points (no schema known) so they always pass through.
pub fn validate_built_in(point: &str, payload: &serde_json::Value) -> Result<(), String> {
    use payloads::*;

    // Per-target context-menu points share one shape.
    if point.starts_with(points::CONTEXT_MENU_PREFIX) {
        return validate::<ContextMenuPayload>(payload);
    }
    match point {
        points::MENU                 => validate::<MenuPayload>(payload),
        points::SIDEBAR              => validate::<SidebarPayload>(payload),
        points::ACTIVITY_BAR         => validate::<ActivityBarPayload>(payload),
        points::COMMAND_PALETTE      => validate::<CommandPayload>(payload),
        points::KEYBINDING           => validate::<KeybindingPayload>(payload),
        points::ICON                 => validate::<IconPayload>(payload),
        points::TREE_STATE           => validate::<TreeStatePayload>(payload),
        points::PANEL_CONTENT        => validate::<PanelContentPayload>(payload),
        points::STATUS_BAR_LEFT
        | points::STATUS_BAR_RIGHT
        | points::TITLE_BAR_LEFT
        | points::TITLE_BAR_RIGHT
        | points::COMMIT_DETAIL_ACTION
        | points::DIFF_TOOLBAR
        | points::COMMIT_FORM_ACTION => validate::<ToolbarItemPayload>(payload),
        points::WELCOME_ACTION       => validate::<WelcomeActionPayload>(payload),
        points::BRANCH_DECORATOR     => validate::<BranchDecoratorPayload>(payload),
        points::FILE_DECORATOR       => validate::<FileDecoratorPayload>(payload),
        _ => Ok(()),
    }
}

fn validate<T: for<'de> Deserialize<'de>>(payload: &serde_json::Value) -> Result<(), String> {
    serde_json::from_value::<T>(payload.clone()).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContribution {
    /// Plugin that contributed this item (for attribution + permission checks).
    pub plugin_name: String,
    /// Contribution point name (e.g. "compile.toolbar").
    pub point:       String,
    /// Unique id within (plugin_name, point). Re-contributing with the same id
    /// replaces the previous payload.
    pub item_id:     String,
    /// Free-form data shaped by the consumer of `point`. Stored as JSON.
    pub payload:     serde_json::Value,
    /// Render order hint. Lower = earlier. Default 100.
    #[serde(default = "default_priority")]
    pub priority:    i32,
    /// Optional gate clause. When present and `whenContext` is supplied at the
    /// consumer side, the contribution is filtered out unless the clause
    /// matches. Promoted from a payload-level convention to a typed top-level
    /// field in Phase 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when:        Option<WhenClause>,
    /// When `true`, the contribution exists in the registry but consumers skip
    /// it at render time. Plugins use this to keep an item registered (so
    /// updates remain idempotent) while temporarily greying it out.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled:    bool,
    /// Optional group label — used by consumers that bucket contributions
    /// (CommandPalette sections, KeybindingsSection groups, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group:       Option<String>,
}

fn default_priority() -> i32 { 100 }
fn is_false(b: &bool) -> bool { !*b }

// ── When-clause (typed top-level filter) ────────────────────────────────────
//
// Mirrors `src/lib/contributions/when.ts` on the frontend: same JSON shape so
// the frontend can keep using the same matcher unchanged. Added in Phase 5
// when `when` was promoted from an informal payload convention to a typed
// top-level contribution field.

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhenClause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind:       Option<StringOrVec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_field: Option<DataFieldMatch>,
    /// Multi-selection scope for context-menu items:
    ///   - `Some(true)`  → only shown when the user multi-selected rows
    ///   - `Some(false)` → only shown for single-row context menus
    ///   - `None`        → shown in both (default, backward compat)
    /// Read by the frontend matcher in `src/lib/contributions/when.ts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi:      Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataFieldMatch {
    pub key:   String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContributionPoint {
    /// Plugin that owns the point (only the owner can declare it; informational).
    pub plugin_name: String,
    /// Point name.
    pub name:        String,
    /// Optional human description (shown in the docs/inspector panels).
    #[serde(default)]
    pub description: Option<String>,
    /// Free-form schema hint — purely documentation, NOT validated at runtime.
    /// Contributors are trusted to send a well-shaped payload.
    #[serde(default)]
    pub schema:      Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Containers — Phase 2
//
// A `Container` is an aggregated UI surface (initially: a modal) whose
// content is built from contributions to two well-known sub-points:
//   <plugin>::<container_id>:category  — the navigation entries
//   <plugin>::<container_id>:section   — the form sections, optionally
//                                         filtered by `payload.category`
//
// Plugins declare a container via `arbor.ui.container.register`, and any
// plugin (host or third-party) contributes categories / sections through
// the regular `arbor.ui.contribute` plumbing. The frontend listens for
// `arbor://container-open` and mounts <ContributableModal> with the
// container id; closing emits `arbor://container-close`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDef {
    /// Globally unique key — `"<plugin>::<id>"`. Built by the registry from
    /// `plugin_name` + `id` so callers don't have to.
    pub key:                                 String,
    /// Plugin that owns the container (only the owner can re-register; the
    /// registry replaces existing entries on the same key).
    pub plugin_name:                         String,
    /// Container id, unique within the owning plugin.
    pub id:                                  String,
    /// Mount type — Phase 2 ships `"modal"`. `"sidebar"` reserved for later.
    #[serde(default = "default_container_kind")]
    pub kind:                                String,
    /// Layout used by the modal. `"tree_nav" | "flat" | "tabbed"`.
    #[serde(default = "default_container_layout")]
    pub layout:                              String,
    pub title:                               String,
    #[serde(default)] pub width:             Option<String>,
    #[serde(default)] pub height:            Option<String>,
    #[serde(default)] pub submit_label:      Option<String>,
    #[serde(default)] pub cancel_label:      Option<String>,
    /// Optional host-level on_save action fired AFTER all section saves.
    #[serde(default)] pub on_save:           Option<String>,
    /// Optional host-level pre-open hook. Fired ONCE when the modal opens,
    /// before categories/sections are read. Lets the host re-contribute its
    /// own categories/sections with fresh state — replacing the per-section
    /// `on_open` fan-out the legacy settings orchestrator used.
    #[serde(default)] pub on_load:           Option<String>,
    /// Override for the category contribution point. Defaults to
    /// `<key>:category`. The settings wrapper sets this to
    /// `<plugin>:settings:category` so existing plugins keep contributing
    /// under the historical single-colon naming.
    #[serde(default)] pub category_point:    Option<String>,
    /// Override for the section contribution point. Defaults to
    /// `<key>:section`.
    #[serde(default)] pub section_point:     Option<String>,
}

fn default_container_kind()   -> String { "modal".to_string() }
fn default_container_layout() -> String { "tree_nav".to_string() }

/// Coalesces bursts of `arbor://contributions-changed` emits per point.
///
/// A plugin that writes 50 contributions to the same point in a tight loop
/// would otherwise fan out 50 frontend reloads. We track which points have an
/// emit pending and drop subsequent requests for the same point until the
/// scheduled emit fires (~16ms later). Emits for *different* points are
/// independent — only same-point bursts collapse.
#[derive(Debug, Default)]
pub struct EventCoalescer {
    pending: Arc<Mutex<HashSet<String>>>,
}

impl EventCoalescer {
    #[allow(dead_code)]
    pub fn new() -> Self { Self::default() }

    /// Schedule a coalesced emit for `point`. If another caller already
    /// scheduled this same point and the timer hasn't fired yet, this is a
    /// no-op — the pending emit will surface this caller's write too.
    pub fn request(&self, handle: &AppHandle, point: &str) {
        {
            let mut g = match self.pending.lock() { Ok(g) => g, Err(_) => return };
            if !g.insert(point.to_string()) { return; }
        }
        let pending = self.pending.clone();
        let handle  = handle.clone();
        let point   = point.to_string();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(COALESCE_WINDOW_MS)).await;
            if let Ok(mut g) = pending.lock() { g.remove(&point); }
            let _ = handle.emit(
                "arbor://contributions-changed",
                serde_json::json!({ "point": point }),
            );
        });
    }
}

impl Clone for EventCoalescer {
    fn clone(&self) -> Self { Self { pending: self.pending.clone() } }
}

/// Shared registry; cloned cheaply (Arc). Mutated under a single Mutex.
#[derive(Debug, Default)]
pub struct ContributionRegistry {
    inner:   Arc<Mutex<RegistryInner>>,
    emitter: EventCoalescer,
}

#[derive(Debug, Default)]
struct RegistryInner {
    /// All contributions, keyed first by point name for fast lookup.
    by_point:   HashMap<String, Vec<PluginContribution>>,
    /// Declared points (informational; lookups never require a declaration).
    points:     HashMap<String, ContributionPoint>,
    /// Container definitions keyed by `"<plugin>::<id>"`. Re-registering on
    /// the same key replaces the previous entry.
    containers: HashMap<String, ContainerDef>,
}

impl ContributionRegistry {
    pub fn new() -> Self { Self::default() }

    /// Add or replace a contribution. Returns `true` if a previous item with
    /// the same (plugin, point, item_id) was replaced.
    ///
    /// Phase 5 — when `point` matches a registered container's section_point,
    /// the payload's `nodes` array is rewritten in place so every field name
    /// gains a `<contributing-plugin>::` prefix. This prevents two plugins
    /// contributing to the same container from colliding on identical field
    /// names; the `<ContributableModal>` strips the prefix at save time
    /// before fanning the slice back to each section's `on_save`.
    pub fn contribute(&self, mut item: PluginContribution) -> bool {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return false };
        if Self::is_container_section_point(&inner.containers, &item.point) {
            if let Some(obj) = item.payload.as_object_mut() {
                if let Some(nodes) = obj.get_mut("nodes") {
                    prefix_node_fields(&item.plugin_name, nodes);
                }
            }
        }
        let bucket = inner.by_point.entry(item.point.clone()).or_default();
        let mut replaced = false;
        if let Some(pos) = bucket.iter().position(|c|
            c.plugin_name == item.plugin_name && c.item_id == item.item_id
        ) {
            bucket[pos] = item;
            replaced = true;
        } else {
            bucket.push(item);
        }
        // Keep sorted by priority for cheap reads. Stable so registration
        // order is preserved within the same priority.
        bucket.sort_by_key(|c| c.priority);
        replaced
    }

    /// True when `point` is the (effective) `section_point` of any registered
    /// container. Computes the default `<key>:section` for unset overrides.
    fn is_container_section_point(
        containers: &HashMap<String, ContainerDef>, point: &str,
    ) -> bool {
        for def in containers.values() {
            let effective = def.section_point.clone()
                .unwrap_or_else(|| format!("{}:section", def.key));
            if effective == point { return true; }
        }
        false
    }

    /// Remove a contribution by (plugin, point, item_id). Returns true on hit.
    pub fn remove(&self, plugin_name: &str, point: &str, item_id: &str) -> bool {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return false };
        if let Some(bucket) = inner.by_point.get_mut(point) {
            if let Some(pos) = bucket.iter().position(|c|
                c.plugin_name == plugin_name && c.item_id == item_id
            ) {
                bucket.remove(pos);
                if bucket.is_empty() { inner.by_point.remove(point); }
                return true;
            }
        }
        false
    }

    /// Drop every contribution from the given plugin. Called on plugin
    /// reload/disable so stale items don't outlive their author.
    pub fn remove_plugin(&self, plugin_name: &str) {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        for bucket in inner.by_point.values_mut() {
            bucket.retain(|c| c.plugin_name != plugin_name);
        }
        inner.by_point.retain(|_, v| !v.is_empty());
        inner.points.retain(|_, p| p.plugin_name != plugin_name);
        inner.containers.retain(|_, def| def.plugin_name != plugin_name);
    }

    pub fn list_for_point(&self, point: &str) -> Vec<PluginContribution> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return Vec::new() };
        inner.by_point.get(point).cloned().unwrap_or_default()
    }

    pub fn list_all(&self) -> Vec<PluginContribution> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return Vec::new() };
        let mut out: Vec<PluginContribution> = inner.by_point.values()
            .flat_map(|v| v.iter().cloned())
            .collect();
        // Stable order across reloads: by point name then existing priority order.
        out.sort_by(|a, b| a.point.cmp(&b.point).then(a.priority.cmp(&b.priority)));
        out
    }

    pub fn declare_point(&self, point: ContributionPoint) {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        inner.points.insert(point.name.clone(), point);
    }

    pub fn list_points(&self) -> Vec<ContributionPoint> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return Vec::new() };
        let mut out: Vec<ContributionPoint> = inner.points.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    // ── Containers (Phase 2) ────────────────────────────────────────────────

    /// Register or replace a container definition. The key is built from
    /// `def.plugin_name` + `def.id` so callers can't desynchronise it.
    pub fn register_container(&self, mut def: ContainerDef) {
        def.key = format!("{}::{}", def.plugin_name, def.id);
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        inner.containers.insert(def.key.clone(), def);
    }

    pub fn get_container(&self, key: &str) -> Option<ContainerDef> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return None };
        inner.containers.get(key).cloned()
    }

    pub fn list_containers(&self) -> Vec<ContainerDef> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return Vec::new() };
        let mut out: Vec<ContainerDef> = inner.containers.values().cloned().collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    }

    // ── Event emission (coalesced) ──────────────────────────────────────────

    /// Emit `arbor://contributions-changed { point }` to the frontend, coalesced
    /// per point. Multiple writers to the same point inside the window collapse
    /// to a single event so a plugin contributing 50 items in a loop produces
    /// one frontend reload, not 50.
    pub fn notify_changed(&self, handle: &Option<AppHandle>, point: &str) {
        if let Some(h) = handle {
            self.emitter.request(h, point);
        }
    }
}

impl Clone for ContributionRegistry {
    fn clone(&self) -> Self {
        Self {
            inner:   self.inner.clone(),
            emitter: self.emitter.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Field-name prefixing for container section nodes (Phase 5)
//
// Container sections may be contributed by *different* plugins to the same
// host container. Without prefixing, two sections with a `name = "host"`
// field would collide in the merged form values. We rewrite each form-DSL
// node so its `name` becomes `<contributing-plugin>::<original>`, and
// rewrite every `field` reference inside `show_if` / `hidden_if` conditions
// the same way so visibility wiring keeps working after the rename.
//
// `<ContributableModal>.handleSave()` is the inverse: it strips the prefix
// before passing the slice back to each section's `on_save`, so the plugin
// itself never sees the namespaced names.
//
// The 7 node containers that recurse: `children`, `tabs[*].children`,
// `steps[*].children`, `nav_children`, `content_children`, `cases[*]`,
// `default`. Anything we don't recognise is left alone.
// ---------------------------------------------------------------------------

fn prefix_node_fields(plugin: &str, value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Array(arr)  => {
            for v in arr.iter_mut() { prefix_node_fields(plugin, v); }
        }
        Value::Object(obj) => {
            // Rename the field-name itself, if any.
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                if !name.starts_with(&format!("{}::", plugin)) {
                    let prefixed = format!("{}::{}", plugin, name);
                    obj.insert("name".to_string(), Value::String(prefixed));
                }
            }
            // Rewrite condition trees referencing field names.
            if let Some(cond) = obj.get_mut("show_if")  { prefix_condition(plugin, cond); }
            if let Some(cond) = obj.get_mut("hidden_if") { prefix_condition(plugin, cond); }
            // Recurse into the well-known child containers.
            for key in &["children", "nav_children", "content_children", "default"] {
                if let Some(child) = obj.get_mut(*key) {
                    prefix_node_fields(plugin, child);
                }
            }
            // tabs[*].children + steps[*].children
            for key in &["tabs", "steps"] {
                if let Some(arr) = obj.get_mut(*key).and_then(|v| v.as_array_mut()) {
                    for entry in arr.iter_mut() {
                        if let Some(children) = entry.get_mut("children") {
                            prefix_node_fields(plugin, children);
                        }
                    }
                }
            }
            // cases is a record: { "<value>": [nodes…] }
            if let Some(cases) = obj.get_mut("cases").and_then(|v| v.as_object_mut()) {
                for (_k, v) in cases.iter_mut() {
                    prefix_node_fields(plugin, v);
                }
            }
        }
        _ => {}
    }
}

/// Rewrite every `field` reference inside a FormCondition tree so it points
/// at the prefixed field name. Conditions can nest under `and` / `or` / `not`.
fn prefix_condition(plugin: &str, value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Array(arr)  => {
            for v in arr.iter_mut() { prefix_condition(plugin, v); }
        }
        Value::Object(obj) => {
            if let Some(field) = obj.get("field").and_then(|v| v.as_str()) {
                if !field.starts_with(&format!("{}::", plugin)) {
                    let prefixed = format!("{}::{}", plugin, field);
                    obj.insert("field".to_string(), Value::String(prefixed));
                }
            }
            for key in &["and", "or", "not"] {
                if let Some(child) = obj.get_mut(*key) {
                    prefix_condition(plugin, child);
                }
            }
        }
        _ => {}
    }
}
