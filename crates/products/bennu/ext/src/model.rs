//! The data an extension is given, and the data it contributes.
//!
//! Everything here is deliberately dumb: spans, strings, and file paths. No Java model,
//! no framework concepts, nothing that would have to be versioned in step with an
//! extension's internals. That is what lets these values travel unchanged from a
//! (possibly out-of-process) extension all the way to the editor.
//!
//! **Offsets are UTF-8 byte offsets** into the file's text — the same convention the rest
//! of the bennu contract uses, mapped to the editor's UTF-16 positions by the frontend.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── What the extension is given ──────────────────────────────────────────────

/// One file handed to an extension during a project scan: its path and its decoded text.
///
/// The host does the walking and the decoding (it owns the project's encoding rules), so
/// an extension never touches the filesystem — which is also what makes it runnable in a
/// sandbox that has no filesystem at all.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub text: String,
}

/// The project as an extension sees it, for [`FrameworkExtension::reindex`].
///
/// Pre-partitioned by role rather than handed over as one undifferentiated list, because
/// every extension starts by making the same split and doing it once is cheaper than
/// doing it per extension.
///
/// [`FrameworkExtension::reindex`]: crate::registry::FrameworkExtension::reindex
pub struct ProjectScan<'a> {
    /// Absolute project root.
    pub root: &'a Path,
    /// Every `.java` source, decoded.
    pub java: &'a [ScannedFile],
    /// Every `.xml` file (config candidates — the extension decides which are its own).
    pub xml: &'a [ScannedFile],
    /// Resource files that carry configuration: `.properties`, `.yml`, `.yaml`.
    pub resources: &'a [ScannedFile],
}

/// One file an extension is being asked about — the buffer as it is *right now*, which
/// for an open editor is the unsaved text, not what is on disk.
pub struct FileCtx<'a> {
    pub path: &'a Path,
    pub source: &'a str,
}

impl FileCtx<'_> {
    /// The file's lowercase extension (`"java"`, `"xml"`, `"yml"`), empty when it has
    /// none. The first thing nearly every extension method branches on.
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default()
    }

    /// The file name (with extension), or an empty string.
    pub fn file_name(&self) -> String {
        self.path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
    }

    /// The path as a forward-slashed string — the form every contributed target uses, so
    /// the frontend compares paths without caring which separator the OS prefers.
    pub fn path_str(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}

// ── What the extension contributes ───────────────────────────────────────────

/// A span the editor should colour, with a *kind* rather than a colour: the extension
/// says what the text is, the theme decides what that looks like.
///
/// Kinds are namespaced by convention (`"spring.placeholder.key"`, `"spring.spel.bean"`),
/// and the frontend maps a kind it doesn't recognise to a neutral class rather than
/// dropping it — an extension can add one without a frontend change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtHighlight {
    pub start: usize,
    pub end: usize,
    pub kind: String,
}

/// A place to jump to: go-to-declaration, a gutter arrow's destination, a catalog row's
/// source site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtTarget {
    /// Absolute path, forward-slashed.
    pub file: String,
    /// Byte offset to place the caret at.
    pub offset: usize,
    /// What the user picks from when there is more than one (`"OrderServiceImpl"`).
    pub label: String,
    /// Secondary line under the label (`"com.acme.order · @Service"`). May be empty.
    pub detail: String,
}

/// A mark in the editor's left gutter — the affordance that makes wiring visible without
/// asking for it, which is most of what "IDE support for a framework" means in practice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtGutterMark {
    /// 1-based line.
    pub line: u32,
    /// Icon key the frontend maps to a glyph (`"bean"`, `"inject"`, `"endpoint"`).
    /// An unknown key renders as a neutral dot rather than nothing.
    pub kind: String,
    /// Tooltip shown on hover.
    pub tooltip: String,
    /// Where clicking it goes. Empty = decorative; one = jump; several = a picker.
    pub targets: Vec<ExtTarget>,
}

/// A hover card contributed by an extension, in the shape the shared editor card renders.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExtHover {
    /// Bold first line (`"app.timeout"`).
    pub title: String,
    /// Monospace signature line under it (`"30"` / `"@Service · OrderServiceImpl"`).
    pub signature: String,
    /// Free prose, may be several lines. Empty when there is nothing to add.
    pub doc: String,
}

/// One row of a catalog — the generic shape behind every list panel an extension backs
/// (beans, endpoints, property keys). Uniform on purpose: one virtualized, filterable
/// list renders all of them, and a new catalog kind costs no frontend work.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExtEntry {
    /// Stable identity within the catalog (a bean name, `"GET /orders/{id}"`).
    pub id: String,
    /// Primary label. Searched.
    pub primary: String,
    /// Secondary detail — an FQCN, a value, a handler. Searched.
    pub secondary: String,
    /// Short classifier rendered as a badge (`"@Service"`, `"GET"`, `"xml"`).
    pub kind: String,
    /// Source site, when the row maps to one.
    pub file: Option<String>,
    /// Byte offset in `file` for the jump.
    pub offset: Option<usize>,
    /// 1-based line, for display.
    pub line: Option<u32>,
    /// Extra flags rendered as small tags (`"primary"`, `"prototype"`, `"@Profile(dev)"`).
    pub tags: Vec<String>,
    /// Sub-rows the UI can expand — a handler's parameters under its route, a bean's injection
    /// points under it.
    ///
    /// Generic on purpose: "a row with detail rows" is the shape half of these lists want, and
    /// putting it here means the one catalog panel renders it for every extension rather than
    /// each growing its own panel. `#[serde(default)]` so a payload without it still
    /// deserializes.
    #[serde(default)]
    pub children: Vec<ExtEntry>,
}

/// A headline number an extension wants surfaced (index inspector / overview cards).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtStat {
    pub label: String,
    pub value: usize,
    /// Catalog kind this stat drills into, when it has one.
    pub catalog: Option<String>,
}
