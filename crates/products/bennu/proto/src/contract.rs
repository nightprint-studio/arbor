//! The Phase-0 wire types.
//!
//! One module for the whole contract — small enough that splitting per-method
//! would just scatter it. Each type is `Serialize + Deserialize` so it round-trips
//! across the framed-stdio boundary and (symmetrically) can be decoded by the FE.
//!
//! Naming note: the *method* names are snake_case (`bennu_open_project`, …) and
//! live on the `bennu-be` handlers; the *types* here are the payloads those methods
//! carry.

use serde::{Deserialize, Serialize};

// ── capabilities ─────────────────────────────────────────────────────────────

/// The domain capabilities Bennu detected for a project (Spike D ruleset). The FE
/// reads this to decide which panels / resolvers to surface; the backend reads the
/// same bitset to gate which index sources it even builds.
///
/// Wire shape: a flat object of booleans (one per capability) plus the raw
/// [`CapabilityHit`] evidence, so the FE can show *why* a capability is on. The
/// canonical bitset + detector live in `bennu-project`; this is only the
/// serialized view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    /// Struts2 / XWork XML action config (`struts.xml`, `*-struts-plugin.xml`).
    pub struts_xml_config: bool,
    /// Struts2 convention plugin (annotation-driven routing).
    pub struts_convention: bool,
    /// JSP taglib / TLD usage (`*.tld` under `WEB-INF`, `web.xml` `<taglib>`).
    pub jsp_taglib_tld: bool,
    /// OGNL value-stack expressions (`%{…}`), follows `struts_xml_config`.
    pub ognl_value_stack: bool,
    /// Apache Tiles view composition (`tiles.xml`, `struts2-tiles-plugin`).
    pub tiles_views: bool,
    /// Spring DI wired by XML (`spring-beans`, root `<beans>`).
    pub spring_xml_di: bool,
    /// Spring DI wired by annotations (`<context:component-scan>`, `@Service`).
    pub spring_annotation_di: bool,
    /// Spring Data repositories (`spring-data-*`, `JpaRepository`).
    pub spring_data_repo: bool,
    /// JPA / Hibernate ORM (`hibernate-core`, `persistence.xml`, `@Entity`).
    pub jpa_hibernate: bool,
    /// MyBatis mappers (`mybatis`, `*Mapper.xml`).
    pub mybatis_mapper: bool,
    /// Plain JDBC-DAO persistence (`spring-jdbc` / JDBC driver + source hit).
    pub jdbc_dao: bool,
    /// Project Lombok (`org.projectlombok:lombok`, `@Data`).
    pub lombok: bool,
    /// Entando / jAPS platform (`org.entando.*` / `com.agiletec.*`, `<wp:*>`).
    pub entando_japs: bool,
    /// The evidence behind each active capability — the signals that tripped it,
    /// so the FE can explain the classification (and mark provisional / C-only ones).
    pub hits: Vec<CapabilityHit>,
}

/// One piece of evidence that activated (or provisionally activated) a capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityHit {
    /// The capability this evidence supports, as the snake_case field name above
    /// (e.g. `"struts_xml_config"`).
    pub capability: String,
    /// Signal tier: `"A"` = dependency coordinate (strongest), `"B"` = config-file
    /// presence, `"C"` = source pattern (corroborating; C-only = provisional).
    pub tier: String,
    /// Human-readable evidence, e.g. `"dependency org.apache.struts:struts2-core"`
    /// or `"file WEB-INF/struts.xml"`.
    pub detail: String,
}

// ── open_project ─────────────────────────────────────────────────────────────

/// Result of `bennu_open_project` — the resolved project model the FE needs to
/// render the workspace header + gate its panels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Absolute path to the project root (the directory holding the root `pom.xml`).
    pub root: String,
    /// Display name (the pom `<name>`, else `<artifactId>`, else the dir name).
    pub name: String,
    /// The Maven modules (the `<modules>` list; empty for a single-module project).
    pub modules: Vec<String>,
    /// The resolved JDK for the project (from the pom, overridable). `None` when it
    /// can't be inferred and no override is set.
    pub jdk: Option<JdkInfo>,
    /// The detected domain capabilities (Spike D ruleset).
    pub capabilities: CapabilitySet,
}

/// The JDK Bennu will resolve classpath sources against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JdkInfo {
    /// The Java language level as declared, e.g. `"1.8"` / `"8"` / `"17"`.
    pub version: String,
    /// Where the version came from: `"maven.compiler.source"`,
    /// `"maven.compiler.target"`, `"compiler-plugin"`, `"toolchains"`,
    /// `"override"`, or `"default"`.
    pub source: String,
}

// ── project_tree ─────────────────────────────────────────────────────────────

/// A node in the project file tree returned by `bennu_project_tree`. Directories
/// carry `children`; files carry none. Lazy-friendly: a directory may be returned
/// with an empty `children` and `expandable = true` so the FE fetches it on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeNode {
    /// The node's own name (the final path segment).
    pub name: String,
    /// Absolute path to the node.
    pub path: String,
    /// `true` for a directory, `false` for a file.
    pub is_dir: bool,
    /// Children for a directory (empty for a file, or for a not-yet-expanded dir).
    pub children: Vec<TreeNode>,
}

// ── read_file ────────────────────────────────────────────────────────────────

/// Result of `bennu_read_file` — the decoded text plus the encoding it was decoded
/// from. Bennu decodes in the project's declared encoding (Cp1252 is common in the
/// legacy target stack), not blindly UTF-8, so the FE must be told which one won.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContents {
    /// The decoded file text (always valid UTF-8 on the wire).
    pub text: String,
    /// The encoding the bytes were decoded from, e.g. `"UTF-8"`, `"Cp1252"`.
    pub encoding: String,
}

// ── completion / diagnostics (Phase-0 stubs) ─────────────────────────────────

/// A single completion candidate returned by `bennu_completion`. Phase 0 returns an
/// empty list; the shape is frozen now so the FE binds against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    /// The text inserted on accept.
    pub label: String,
    /// Kind tag, e.g. `"method"` / `"field"` / `"class"` / `"keyword"`.
    pub kind: String,
    /// Optional right-aligned detail (a signature, a type).
    pub detail: Option<String>,
}

/// A single diagnostic returned by `bennu_diagnostics`. Phase 0 returns an empty
/// list; the shape is frozen now so the FE binds against it. Byte offsets, not
/// line/col — the FE maps them against the buffer it already has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The message shown to the user.
    pub message: String,
    /// Severity: `"error"` | `"warning"` | `"info"` | `"hint"`.
    pub severity: String,
    /// Start byte offset in the file.
    pub start: usize,
    /// End byte offset (exclusive) in the file.
    pub end: usize,
}

// ── rename (docs §5 #10-12) ──────────────────────────────────────────────────

/// One concrete rename edit: replace `[start, end)` in `file` with `new_text`. Byte
/// offsets (like [`Diagnostic`]) — the FE maps them against the buffer it holds, applies
/// the edit through CodeMirror (so undo works), and can guard on `old` still matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameEdit {
    /// Absolute path (forward slashes) of the file to edit.
    pub file: String,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The replacement text.
    pub new_text: String,
    /// The exact text currently at `[start, end)` — a stale-buffer guard for the FE.
    pub old: String,
    /// Why this edit exists: `"declaration"` | `"reference"` | `"import"` |
    /// `"spring-bean"` | `"local"`. Drives the preview grouping.
    pub reason: String,
    /// True when the edit is inferred/heuristic (a method use-site where an overload could
    /// collapse). The FE surfaces these for review, never auto-applies as if exact.
    pub inferred: bool,
}

/// The edits for one file (the preview list the FE renders per file, in offset order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameFileEdits {
    /// Absolute path (forward slashes) of the file.
    pub file: String,
    /// The edits in this file, sorted by start offset.
    pub edits: Vec<RenameEdit>,
}

/// Result of `bennu_rename_plan` — the PREVIEW the FE renders before the user confirms.
/// `bennu_rename_apply` returns the same edits flattened (the FE applies them). `None` on
/// the wire (a bare object absent) when the caret isn't renameable or the engine is still
/// building.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenamePreview {
    /// The old identifier under the caret.
    pub old_name: String,
    /// The requested new name.
    pub new_name: String,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"local `x`"`, …).
    pub target_label: String,
    /// The edits, grouped by file.
    pub files: Vec<RenameFileEdits>,
    /// Total number of edit sites (across all files).
    pub total_edits: usize,
    /// Whether any edit is `inferred` (the FE nudges review before applying).
    pub has_inferred: bool,
}

// ── build / run (docs §4 "il fondo") ─────────────────────────────────────────

/// A structured build diagnostic parsed out of `javac` / `mvn` compiler output by
/// `bennu_build`. Unlike the editor [`Diagnostic`] (byte offsets over a buffer the FE
/// already holds), a build diagnostic comes from a compiler that reports `file:line:col`
/// with no buffer context — so it carries the file path + 1-based line/col instead. The
/// FE navigates to `file` and highlights `line:col`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildDiagnostic {
    /// Path to the offending file (as the compiler emitted it), when the line had one.
    pub file: Option<String>,
    /// 1-based line number, when present.
    pub line: Option<u32>,
    /// 1-based column, when present.
    pub col: Option<u32>,
    /// `"error"` | `"warning"` | `"note"`.
    pub severity: String,
    /// The human-readable message.
    pub message: String,
}

/// Result of `bennu_build` — the parsed diagnostics plus enough context for the FE to
/// render the Build panel (which tool ran, whether it succeeded). The raw log is
/// streamed as `arbor://bennu/build-output` events, not returned inline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildResult {
    /// The tool that ran: `"mvn"` or `"javac"` (the fallback).
    pub tool: String,
    /// Whether the underlying process exited 0.
    pub ok: bool,
    /// Structured diagnostics parsed from the compiler output.
    pub diagnostics: Vec<BuildDiagnostic>,
}

/// Result of `bennu_run` — the id of the launched run, used to correlate the
/// `arbor://bennu/run-output` / `arbor://bennu/run-exit` event stream and to
/// `bennu_cancel_run`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunHandle {
    /// The run id (unique per launch this session).
    pub run_id: String,
    /// The resolved main class the run launched.
    pub main_class: String,
}
