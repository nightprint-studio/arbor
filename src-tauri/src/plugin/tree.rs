//! Tree-state storage for `kind="tree"` plugin sidebars.
//!
//! A tree sidebar is owned by one plugin (the "host"). The host pushes the
//! current set of nodes via `arbor.ui.tree.set(sidebar_id, nodes)`. The same
//! call dual-writes the snapshot into the unified `ContributionRegistry` under
//! point `"arbor:tree-state"` (item_id = sidebar_id), which is what the
//! frontend reads — this `TreeStore` is kept as a backend-internal cache so
//! `arbor.ui.tree.get(sidebar_id)` from Lua can return a typed snapshot
//! without round-tripping through the contribution registry.
//!
//! Nodes are intentionally schema-light so the consumer can shape them per use
//! case — e.g. compile-action distinguishes `kind = "section" | "module" |
//! "lifecycle_phase" | "runnable" | "static"` and the frontend interprets them
//! cosmetically (icon override, decorator, default action).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Deserializer, Serialize};

/// Lenient deserializer for `children` (and any other Vec<TreeNode>-shaped
/// list pushed from Lua). mlua's serde bridge encodes empty Lua tables as
/// JSON objects `{}` rather than arrays `[]`, because there's no way to tell
/// the two apart at the table level. Without this helper every leaf node
/// (which has `children = {}` in Lua) would fail to deserialize, and `serde`
/// would propagate the error all the way up — collapsing the whole snapshot
/// to an empty Vec via `unwrap_or_default()` on the caller side.
///
/// Acceptance matrix:
///   · missing / null            → empty Vec (via `#[serde(default)]`)
///   · `[]`                      → empty Vec
///   · `{}` (empty object)       → empty Vec  ← Lua's empty-table case
///   · `[…children…]`            → parsed normally
///   · anything else             → empty Vec (silent — defensive against
///                                  partial drafts so one bad node can't
///                                  blow up the whole sidebar)
fn deserialize_lenient_children<'de, D>(d: D) -> Result<Vec<TreeNode>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(_) => {
            serde_json::from_value::<Vec<TreeNode>>(v).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Object(map) if map.is_empty() => Ok(Vec::new()),
        // Truthy object that isn't a sequence — coerce silently. The frontend
        // would have nothing useful to render with it anyway.
        _ => Ok(Vec::new()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeNode {
    /// Stable id within the parent. Used by the frontend to remember
    /// expand/select state across `set` calls. Required.
    pub id:       String,
    /// Display label.
    pub label:    String,
    /// Optional Lucide name, emoji, or `"plugin:<plugin>:<icon_id>"` ref into
    /// the custom icon registry. Resolved by `PluginIcon.svelte`.
    #[serde(default)]
    pub icon:     Option<String>,
    /// Optional small text shown right of the label (counts, badges, status).
    #[serde(default)]
    pub badge:    Option<String>,
    /// Optional badge color hint: `"info" | "success" | "warning" | "error" |
    /// "muted" | "accent"`.
    #[serde(default)]
    pub badge_kind: Option<String>,
    /// Free-form classification consumed by the host. Common values used by
    /// compile-action: `section` (gray header row), `module`, `lifecycle_phase`,
    /// `runnable`, `static`. Defaults to `"static"`.
    #[serde(default = "default_kind")]
    pub kind:     String,
    /// When non-empty, the row is selectable / right-clickable and the
    /// frontend fires `tree:select` / `tree:context_menu` with this node.
    #[serde(default)]
    pub selectable: bool,
    /// Default expanded state on first display. Subsequent renders use the
    /// user's saved preference if any.
    #[serde(default)]
    pub expanded:   bool,
    /// Fired when the row is double-clicked (or activated via keyboard). The
    /// payload is `{ node_id, data }`.
    #[serde(default)]
    pub default_action: Option<String>,
    /// Fired on every single-click selection of this row (after the local
    /// selection state has been updated). Payload `{ node_id, data }`. Use
    /// this for "show-details-on-select" UX where double-click would be a
    /// friction wall. Independent from `default_action` — both can be set.
    #[serde(default)]
    pub selection_action: Option<String>,
    /// Free-form data attached to the node — passed back to the host on every
    /// action / context_menu / dependency-provider invocation.
    #[serde(default)]
    pub data:     serde_json::Value,
    /// Phase 6.2 — opt-in: the row becomes an HTML5 drag source. The frontend
    /// initiates a drag with `{node_id, data}` as the payload. The host plugin
    /// owns the semantics (entity reparent, file move, …) via `drop_action`
    /// on the snapshot.
    #[serde(default)]
    pub draggable:  bool,
    /// Phase 6.2 — opt-in: the row accepts drops. Combined with another row's
    /// `draggable`, a drop fires `drop_action` with `{source_id, source_data,
    /// target_id, target_data}`. A node can be both draggable and a drop
    /// target (e.g. entity tree — drop A on B → A becomes child of B).
    #[serde(default)]
    pub drop_target: bool,
    /// Optional hover-tooltip for the row. Useful when the visible
    /// label/badge are truncated and the full text only fits in a
    /// tooltip — typical case is a Rust type path on a component leaf
    /// where the label is the short name and the badge is the path.
    #[serde(default)]
    pub tooltip: Option<String>,
    /// Children nodes. May be empty.
    #[serde(default, deserialize_with = "deserialize_lenient_children")]
    pub children: Vec<TreeNode>,
}

fn default_kind() -> String { "static".to_string() }

/// One segment of a sidebar breadcrumb band. Rendered as a clickable chip
/// when `action` is non-empty; otherwise it's a static label (typically the
/// last/current segment).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BreadcrumbSegment {
    /// Display label (e.g. a bucket name, a folder segment).
    pub label:  String,
    /// Optional Lucide name, emoji, or `plugin:<plugin>:<icon_id>` reference.
    #[serde(default)]
    pub icon:   Option<String>,
    /// Plugin action fired on click. When empty/nil the chip renders as
    /// non-interactive (last segment / current location).
    #[serde(default)]
    pub action: Option<String>,
    /// Free-form data forwarded to the action handler.
    #[serde(default)]
    pub data:   serde_json::Value,
    /// Optional small text shown right of the label (e.g. "current"). Cosmetic.
    #[serde(default)]
    pub badge:  Option<String>,
    /// Optional tooltip shown on hover.
    #[serde(default)]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeSnapshot {
    /// Plugin that owns the sidebar (the only writer).
    pub plugin_name: String,
    /// Sidebar id (matches `PluginSidebarSection.id`).
    pub sidebar_id:  String,
    /// Optional title shown in the header (overrides the section's `label`).
    #[serde(default)]
    pub title:       Option<String>,
    /// Optional breadcrumb band rendered between the section header and the
    /// tree body. When empty the band is hidden.
    #[serde(default)]
    pub breadcrumb:  Vec<BreadcrumbSegment>,
    /// Optional: when set, the breadcrumb shows a pencil affordance and the
    /// user can flip it into a text input to type a path directly. On commit
    /// (Enter) the named plugin action is fired with `{ path }` in ctx. The
    /// plugin is responsible for parsing the path and updating its state.
    #[serde(default)]
    pub breadcrumb_edit_action: Option<String>,
    /// Placeholder shown inside the edit input. Optional, plugin-specific.
    #[serde(default)]
    pub breadcrumb_edit_placeholder: Option<String>,
    /// Phase 6.2 — fired when a `draggable` node is dropped on a
    /// `drop_target` node. Payload: `{ source_id, source_data, target_id,
    /// target_data }`. Without this the tree silently ignores drops even if
    /// individual nodes opt in.
    #[serde(default)]
    pub drop_action: Option<String>,
    /// The current node tree. May be empty (renders an empty-state placeholder).
    pub nodes:       Vec<TreeNode>,
    /// Monotonic version, bumped on every `set`. Lets the frontend detect
    /// stale snapshots when events arrive out of order.
    #[serde(default)]
    pub version:     u64,
}

#[derive(Debug, Default)]
pub struct TreeStore {
    inner: Arc<Mutex<StoreInner>>,
}

#[derive(Debug, Default)]
struct StoreInner {
    /// Keyed by `<plugin_name>::<sidebar_id>` to namespace across plugins.
    snapshots: HashMap<String, TreeSnapshot>,
    next_version: u64,
}

impl TreeStore {
    pub fn new() -> Self { Self::default() }

    fn key(plugin: &str, sidebar: &str) -> String { format!("{plugin}::{sidebar}") }

    /// Replace the snapshot for the given (plugin, sidebar). Returns the new
    /// version number.
    pub fn set(
        &self,
        plugin:     &str,
        sidebar:    &str,
        title:      Option<String>,
        breadcrumb: Vec<BreadcrumbSegment>,
        breadcrumb_edit_action:      Option<String>,
        breadcrumb_edit_placeholder: Option<String>,
        drop_action: Option<String>,
        nodes:      Vec<TreeNode>,
    ) -> u64 {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return 0 };
        inner.next_version = inner.next_version.wrapping_add(1);
        let version = inner.next_version;
        let snap = TreeSnapshot {
            plugin_name: plugin.to_string(),
            sidebar_id:  sidebar.to_string(),
            title,
            breadcrumb,
            breadcrumb_edit_action,
            breadcrumb_edit_placeholder,
            drop_action,
            nodes,
            version,
        };
        inner.snapshots.insert(Self::key(plugin, sidebar), snap);
        version
    }

    pub fn get(&self, plugin: &str, sidebar: &str) -> Option<TreeSnapshot> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return None };
        inner.snapshots.get(&Self::key(plugin, sidebar)).cloned()
    }

    pub fn remove_plugin(&self, plugin: &str) {
        let mut inner = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        inner.snapshots.retain(|_, snap| snap.plugin_name != plugin);
    }

    pub fn list(&self) -> Vec<TreeSnapshot> {
        let inner = match self.inner.lock() { Ok(g) => g, Err(_) => return Vec::new() };
        inner.snapshots.values().cloned().collect()
    }
}

impl Clone for TreeStore {
    fn clone(&self) -> Self { Self { inner: self.inner.clone() } }
}

// ---------------------------------------------------------------------------
// Custom icon registry — plugin-supplied SVG strings keyed by id.
// ---------------------------------------------------------------------------

/// Plugin-registered SVG icons. Each plugin owns a namespace `<plugin>:<id>`
/// referenced as `"plugin:<plugin>:<id>"` in any `icon` field.
#[derive(Debug, Default)]
pub struct IconRegistry {
    inner: Arc<Mutex<HashMap<String, String>>>, // key = "<plugin>:<id>", value = raw SVG
}

impl IconRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register(&self, plugin: &str, id: &str, svg: String) {
        let mut g = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        g.insert(format!("{plugin}:{id}"), svg);
    }

    pub fn remove_plugin(&self, plugin: &str) {
        let prefix = format!("{plugin}:");
        let mut g = match self.inner.lock() { Ok(g) => g, Err(_) => return };
        g.retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn snapshot(&self) -> HashMap<String, String> {
        let g = match self.inner.lock() { Ok(g) => g, Err(_) => return HashMap::new() };
        g.clone()
    }
}

impl Clone for IconRegistry {
    fn clone(&self) -> Self { Self { inner: self.inner.clone() } }
}
