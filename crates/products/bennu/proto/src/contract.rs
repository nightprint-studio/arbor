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

// ── write_file ───────────────────────────────────────────────────────────────

/// Result of `bennu_write_file` — the encoding the buffer was actually encoded in on
/// save. Bennu writes in the project's resolved encoding (the round-trip inverse of
/// [`FileContents::encoding`]); a char the declared encoding can't represent falls back
/// to UTF-8, so the FE is told which encoding truly landed on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteResult {
    /// The encoding the bytes were written in, e.g. `"UTF-8"`, `"Cp1252"`.
    pub encoding: String,
}

// ── references / find-usages (docs §5 #7) ────────────────────────────────────

/// One resolved use site returned by `bennu_references`. Byte offsets (like
/// [`Diagnostic`] / [`RenameEdit`]) plus 1-based line/col + a trimmed source-line
/// preview, so the FE can both navigate to the span and render a results list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageHit {
    /// Absolute path (forward slashes) of the file the use is in.
    pub file: String,
    /// Start byte offset of the referencing identifier.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of the reference.
    pub line: usize,
    /// 1-based column of the reference.
    pub col: usize,
    /// The trimmed source line text, for the results-list preview.
    pub preview: String,
}

/// Result of `bennu_references` — the declaration the caret resolved to (as a human
/// label) plus its use sites across the project. `None` on the wire when no project owns
/// the file, its index is still building, or the caret isn't on a referenceable symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsagesResult {
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"type com.x.Foo"`).
    pub target_label: String,
    /// The resolved use sites across the project.
    pub usages: Vec<UsageHit>,
}

// ── go-to-declaration (Ctrl+Click / Ctrl+B) ──────────────────────────────────

/// Result of `bennu_declaration` — the declaration site the symbol under the caret resolves
/// to (methods / fields / locals / classes), for the FE's go-to-declaration. `None` on the
/// wire (a bare object absent) when no project owns the file, the index is still building,
/// the caret isn't on a resolvable symbol, or the declaration lives in a JDK / dep-jar (no
/// project source to open).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclarationTarget {
    /// Absolute path (forward slashes) of the file declaring the symbol — reported the same
    /// way [`UsageHit::file`] is (the FE keys files by forward-slash paths).
    pub file: String,
    /// Start byte offset of the declaration NAME token in `file`.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of the declaration name in `file`.
    pub line: u32,
    /// 1-based column of the declaration name in `file`.
    pub col: u32,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"field count"`,
    /// `"class com.x.Order"`, `"local `x`"`).
    pub label: String,
}

// ── hover (editor hover card) ────────────────────────────────────────────────

/// Result of `bennu_hover` — the hover card for the symbol under the caret. `None` on the
/// wire (a bare object absent) when no project owns the file, its index is still building,
/// or the caret isn't on a symbol we can classify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    /// The signature line: a member's raw signature (or a synthesized `name(…)` fallback),
    /// or a type's dotted FQCN.
    pub signature: String,
    /// `"method"` | `"field"` | `"class"` | `"interface"` | `"enum"` (best-effort — types
    /// are currently reported as `"class"`).
    pub kind: String,
    /// The owning type's dotted FQCN for a member; `None` for a type.
    pub container: Option<String>,
    /// A best-effort leading Javadoc for a project declaration (the `/** … */` block above
    /// it, markers stripped, capped ~600 chars). `None` for a JDK / dep-jar symbol or a
    /// declaration with no Javadoc.
    pub doc: Option<String>,
}

// ── inherited members (Structure panel's "Inherited" bucket) ─────────────────

/// One inherited ("super") member returned by `bennu_inherited_members` — a method or field
/// declared on a SUPERCLASS / INTERFACE of the queried type (not the type's own members).
/// Feeds the Structure panel's lazy "Inherited" bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InheritedMember {
    /// `"method"` | `"field"`.
    pub kind: String,
    /// The member's simple name.
    pub name: String,
    /// A readable detail: the return type (methods) / the field type. `None` when unrecorded.
    pub detail: Option<String>,
    /// `"public"` | `"protected"` | `"private"` | `"package"`.
    pub visibility: String,
    /// The dotted FQCN of the class / interface that declares the member.
    pub declaring_type: String,
    /// The project-source declaration site (file + 1-based line), or `None` when the
    /// declaring type is a JDK / jar type (no project source to open) — like
    /// go-to-declaration for a JDK symbol.
    pub source: Option<InheritedSource>,
}

/// Where an inherited member's declaring type lives, when it resolves to PROJECT source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InheritedSource {
    /// Absolute path (forward slashes) of the project file declaring the member's type.
    pub file_path: String,
    /// 1-based line of that type's declaration.
    pub line: i64,
}

// ── index stats (index inspector) ─────────────────────────────────────────────

/// Result of `bennu_index_stats` — a cheap snapshot of the per-project index for the
/// index-inspector panel. Never errors just because the index isn't built yet: an
/// unbuilt project reports zeros + `ready = false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStats {
    /// Declared project types in the symbol index (0 until the build lands).
    pub types: usize,
    /// Declared project members (methods + fields) in the symbol index (0 until built).
    pub members: usize,
    /// The resolved JDK language level the project was opened at (e.g. `"1.8"`, `"17"`).
    pub jdk_version: String,
    /// Number of classpath jars resolved for the project. Currently always 0 (not yet
    /// tracked in the slot).
    pub jar_count: usize,
    /// Struts/XWork actions in the config graph (0 when no config / not built).
    pub actions: usize,
    /// Spring beans in the config graph (0 when no config / not built).
    pub beans: usize,
    /// Config-graph relations / edges (0 when no config / not built).
    pub relations: usize,
    /// Whether the project's index build (provider + rename engine) has finished.
    pub ready: bool,
}

// ── encoding report (non-compliant source files) ─────────────────────────────

/// One source file whose bytes were NOT valid in the project's declared (Maven
/// `sourceEncoding`) encoding, produced by `bennu_encoding_report`. Bennu recovered + indexed
/// it anyway (so its classes aren't lost), but records it here so a future UI can list the
/// files that need their real encoding sorted out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingIssue {
    /// Absolute path (forward slashes) of the non-compliant source.
    pub file: String,
    /// The encoding the project declared (and that didn't fit the bytes), e.g. `"Cp1252"`.
    pub declared_encoding: String,
    /// The encoding actually used to recover the text (`"UTF-8"` / `"windows-1252"`).
    pub decoded_as: String,
}

// ── JDK status (titlebar / Problems diagnostics) ─────────────────────────────

/// How the project's JDK resolved, produced by `bennu_jdk_status`. Drives the FE's JDK
/// diagnostics: a titlebar warning when no JDK is installed at all, and a Problems entry
/// when a fallback JDK was used (the exact language level the project targets isn't
/// installed, so the standard library is resolved against a different JDK).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JdkStatus {
    /// The Java language level the project targets (`None` if the version was unparseable).
    pub requested_major: Option<u32>,
    /// Absolute path of the JDK home that would be used (exact match or fallback), or `None`
    /// when no JDK is installed.
    pub resolved_home: Option<String>,
    /// The language level of the resolved JDK, if any.
    pub resolved_major: Option<u32>,
    /// True when a JDK of the exact requested level was found (no fallback).
    pub exact: bool,
    /// True when at least one JDK is installed (an exact match or a fallback candidate).
    pub any_installed: bool,
}

// ── index entries (index inspector per-kind lists) ───────────────────────────

/// One row in the index inspector's per-kind entry list, produced by
/// `bennu_index_entries` for every non-`types` kind (`members` / `jars` / `jdk` /
/// `beans` / `actions` / `relations`; `types` is served by `bennu_class_index`). A
/// deliberately generic shape so a single virtualized+filterable FE list renders every
/// kind: a `primary` label + `secondary` detail (both searched), plus an OPTIONAL
/// openable source site (`file` + 1-based `line`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Primary label — the entry's name (member simple name / jar filename / bean id /
    /// action qualified name / edge label / JDK label). Searched.
    pub primary: String,
    /// Secondary detail — owning FQCN + signature / abs path / bean class / resolved
    /// class / relation kind / JDK version. Also searched. May be empty.
    pub secondary: String,
    /// Absolute path (forward slashes) of an openable source site, or `None` when the
    /// entry has no navigable location (a jar, a JDK module, a member with no source).
    pub file: Option<String>,
    /// 1-based line to jump to when `file` is `Some`; `None` otherwise.
    pub line: Option<i64>,
}

// ── class index (Go to Class) ────────────────────────────────────────────────

/// One declared Java type in the "Go to Class" navigator, produced by
/// `bennu_class_index`. A fresh source scan (no persisted index required): each
/// declared type (class / interface / enum, incl. nested) becomes one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassEntry {
    /// Fully-qualified, dotted class name (`com.acme.Order`).
    pub fqcn: String,
    /// The simple (unqualified) type name (`Order`).
    pub simple: String,
    /// Absolute path (forward slashes) of the source file declaring the type.
    pub file: String,
    /// 1-based line of the type declaration.
    pub line: usize,
}

// ── TODO scan (TODO tool window) ─────────────────────────────────────────────

/// One TODO-style marker hit found by `bennu_todos`. A line-oriented scan of the
/// project sources — one entry per matched marker line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoItem {
    /// Absolute path (forward slashes) of the file the marker is in.
    pub file: String,
    /// 1-based line of the marker.
    pub line: usize,
    /// The marker word: `"TODO"` | `"FIXME"` | `"XXX"` | `"HACK"`.
    pub kind: String,
    /// The trimmed remainder of the line after the marker (capped ~200 chars).
    pub text: String,
}

// ── find in files (project-wide text search) ─────────────────────────────────

/// One matching line found by `bennu_find_in_files`. A line-oriented, project-wide text
/// scan — one entry per matched line (the first match on the line drives `col`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindHit {
    /// Absolute path (forward slashes) of the file the match is in.
    pub file: String,
    /// 1-based line of the match.
    pub line: usize,
    /// 1-based column (char count) of the first match on the line.
    pub col: usize,
    /// The trimmed matching line, for the results-list preview (capped ~300 chars).
    pub preview: String,
}

// ── spell-check (editor niceties) ────────────────────────────────────────────

/// One misspelled sub-word occurrence returned by `bennu_spellcheck`. Byte offsets (like
/// [`Diagnostic`]) into the checked `source` — the FE underlines `[start, end)` and offers
/// `suggestions`. Only the words the user *authored* (declaration-name identifiers, split
/// by case, + comment words) are checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellHit {
    /// Start byte offset of the misspelled sub-word within the source.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// The offending sub-word (as it appears in the source).
    pub word: String,
    /// Up to ~5 suggested corrections (empty when none available).
    pub suggestions: Vec<String>,
}

/// The spell-check dictionary status returned by `bennu_spell_status` /
/// `bennu_download_dictionaries` — whether any Hunspell dictionary is installed and which
/// of `en_US` / `it_IT` are on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellStatus {
    /// True when at least one dictionary (`en_US` or `it_IT`) is present on disk.
    pub installed: bool,
    /// The installed languages (subset of `["en_US", "it_IT"]`).
    pub languages: Vec<String>,
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

// ── run configurations (per-repo `[bennu.run]`, IntelliJ-style run targets) ───

/// One `key=value` environment-variable entry of a [`RunConfig`]. Serialized as a
/// TOML array-of-tables row (`[[bennu.run.configs.env]]`) so the whole set round-trips
/// through `<repo>/.arbor/config.toml` losslessly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    /// The variable name.
    pub key: String,
    /// The variable value (verbatim).
    pub value: String,
}

/// A single NAMED run configuration — the IntelliJ-style run target the FE's
/// run-configuration editor edits and the `▶ Run` / `Shift+F10` path launches. Maps
/// 1:1 to a `[[bennu.run.configs]]` TOML entry (`env` as an array-of-tables, args as
/// raw single-line strings the FE splits into an argv at launch).
///
/// `id` is STABLE across restarts — the FE generates it and it is persisted verbatim;
/// the backend never re-assigns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    /// Stable id (FE-generated, persisted verbatim — the map/selection key).
    pub id: String,
    /// Display name of the configuration.
    pub name: String,
    /// The fully-qualified main class to launch.
    pub main_class: String,
    /// Program arguments (passed after the main class), a raw single-line string.
    pub program_args: String,
    /// JVM arguments (`-Xmx…`, `-D…`), a raw single-line string.
    pub vm_args: String,
    /// Working directory; empty = the project root.
    pub working_dir: String,
    /// Environment-variable entries applied to the launched process.
    pub env: Vec<EnvVar>,
}

/// Result of `bennu_get_run_config` — the per-repo run-config bundle (the ordered list
/// plus which one is active). A fresh repo (no `[bennu.run]` section) yields
/// `{ configs: [], active_id: null }`. The payload `bennu_set_run_config` persists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RunConfigSet {
    /// The ordered run configurations for the repo.
    pub configs: Vec<RunConfig>,
    /// Id of the active config (what `▶ Run` launches), or `None` when none/empty.
    pub active_id: Option<String>,
}

// ── main-class discovery (`bennu_main_classes`) ──────────────────────────────

/// One class declaring a `public static void main(String[])` entry point, found by
/// `bennu_main_classes` scanning the project sources. Feeds the run-config editor's
/// main-class picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainClassEntry {
    /// Fully-qualified, dotted class name of the enclosing type (`com.acme.App`).
    pub fqcn: String,
    /// Absolute path (forward slashes) of the source file declaring it, when known.
    pub source_file: Option<String>,
    /// The Maven module the source lives in (relative to the project root), when the
    /// project is multi-module; `None` for a single-module project.
    pub module: Option<String>,
}
