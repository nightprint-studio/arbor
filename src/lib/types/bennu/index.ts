/**
 * Bennu (Java editor) FE types — mirror the BE↔FE contract (`bennu-proto`)
 * field-for-field in **snake_case**. The Rust wire shape is authoritative; see
 * `crates/products/bennu/proto/src/contract.rs`. Types only, no UI/state. The
 * commands live in `$lib/ipc/bennu`.
 */

/** The JDK Bennu resolves classpath sources against (`ProjectInfo.jdk`). */
export interface JdkInfo {
  /** Java language level as declared, e.g. `1.8` / `8` / `17`. */
  version: string;
  /** Where it came from: `maven.compiler.source` | `…target` | `compiler-plugin` |
   *  `toolchains` | `override` | `default`. */
  source: string;
}

/** One piece of evidence that activated (or provisionally activated) a capability. */
export interface CapabilityHit {
  /** The capability field name it supports (e.g. `struts_xml_config`). */
  capability: string;
  /** Tier: `A` dependency coord (strongest) · `B` config file · `C` source pattern
   *  (corroborating; C-only = provisional). */
  tier: string;
  /** Human-readable evidence, e.g. `dependency org.apache.struts:struts2-core`. */
  detail: string;
}

/** The domain capabilities detected for a project (Spike-D ruleset): a flat bitset
 *  of booleans plus the raw evidence. The FE gates panels/resolvers on these; the BE
 *  gates which index sources it builds. */
export interface CapabilitySet {
  struts_xml_config: boolean;
  struts_convention: boolean;
  jsp_taglib_tld: boolean;
  /** The project has JSP views at all (`*.jsp` / `*.jspf` / `*.tag`) — what the JSP-only
   *  tooling (Forms, Tomcat deploy) is gated on. Weaker than {@link jsp_taglib_tld}. */
  jsp_views: boolean;
  ognl_value_stack: boolean;
  tiles_views: boolean;
  spring_xml_di: boolean;
  spring_annotation_di: boolean;
  spring_data_repo: boolean;
  jpa_hibernate: boolean;
  mybatis_mapper: boolean;
  jdbc_dao: boolean;
  lombok: boolean;
  entando_japs: boolean;
  hits: CapabilityHit[];
}

/**
 * Which manifest governs a project root — the one fact that decides how much of Bennu
 * applies to it. `maven` gets the whole Java stack (symbol index, capability detection,
 * JDK, validation, Generate…); `cargo` gets the editor (tree, go-to-file,
 * find-in-files, highlighting, `cargo check`) and nothing that would need a Java index
 * to be true. Gate Java-only UI on this rather than on `jdk === null`.
 */
export type ProjectKind = 'maven' | 'cargo';

/** An opened project (`bennu_open_project`) — Maven or Cargo, see {@link ProjectKind}. */
export interface ProjectInfo {
  /** Absolute project root folder (holds the root `pom.xml` / `Cargo.toml`). */
  root: string;
  /** Display name (pom `<name>` / Cargo `package.name`, else the folder name). */
  name: string;
  /** Sub-projects: Maven `<modules>` or Cargo workspace members (globs expanded).
   *  Empty for a single-module / single-crate project. */
  modules: string[];
  /** Which manifest governs the root. Absent in an older payload → treat as `maven`. */
  kind: ProjectKind;
  /** Resolved JDK, or `null` when it can't be inferred and no override is set — always
   *  `null` for a Cargo project. */
  jdk: JdkInfo | null;
  /** The detected domain capabilities. All-false for a Cargo project. */
  capabilities: CapabilitySet;
  /** The project's declared source encoding — the pom `sourceEncoding`, else the config
   *  default (e.g. `UTF-8`, `Cp1252`). Shown in the app status bar; an individual file's
   *  decoded encoding (which can differ) rides on the read result / editor footer.
   *  Always `UTF-8` for a Cargo project (Rust source is UTF-8 by definition). */
  source_encoding: string;
}

/** One node of the project file tree (`bennu_project_tree`). Directories carry
 *  `children`; files carry an empty array. `is_dir` distinguishes a not-yet-expanded
 *  directory (empty `children`) from a file. */
export interface TreeNode {
  /** Display name (final path segment). */
  name: string;
  /** Absolute path. */
  path: string;
  /** `true` for a directory, `false` for a file. */
  is_dir: boolean;
  /** Children for a directory (empty for a file or a not-yet-expanded dir). */
  children: TreeNode[];
}

/** Result of `bennu_read_file`: the decoded text and the encoding it was decoded
 *  from (Cp1252 is common in the legacy target stack, so the FE is told which won). */
export interface ReadFileResult {
  text: string;
  /** Encoding label, e.g. `UTF-8`, `Cp1252`. */
  encoding: string;
  /** The on-disk state this text came from — see {@link FileStamp}. Hand it back to
   *  {@link import('$lib/ipc/bennu').writeFile} so a save refuses instead of overwriting a
   *  file something else changed meanwhile. */
  stamp: string;
}

/** Result of `bennu_write_file`: the encoding the text was actually encoded with
 *  (the project encoding, or `UTF-8` if that couldn't represent a character). */
export interface WriteResult {
  encoding: string;
  /** The on-disk state just written — the new baseline for the next save's check. */
  stamp: string;
}

/**
 * One file's on-disk fingerprint (`bennu_file_stamps`) — **opaque**: compare stamps for
 * equality, never parse one.
 *
 * A stat, not a content hash, so polling every open tab costs nothing. `stamp` is `''` and
 * `exists` is `false` for a file that is gone — which the caller must treat as "changed"
 * for the purpose of warning, but *not* as a reason to block the save that recreates it.
 */
export interface FileStamp {
  /** Absolute path, echoed back so a batch reply needs no ordering contract. */
  file: string;
  /** The stamp, or `''` when the file is gone / unreadable. */
  stamp: string;
  /** Whether the file exists on disk at all. */
  exists: boolean;
}

/** One completion candidate (`bennu_completion`). Phase 0 returns `[]`. */
export interface CompletionItem {
  /** Text inserted on accept. */
  label: string;
  /** Kind tag for the icon/grouping (`method`, `field`, `class`, `keyword`, …). */
  kind: string;
  /** Optional signature / type detail shown right of the label. */
  detail?: string;
  /** For a type-name completion whose simple name resolves to a SINGLE class: the fully-qualified
   *  name to auto-import on accept (when the auto-import setting is on). Absent for member
   *  completions and ambiguous names. */
  auto_import?: string;
}

/** Severity of a {@link Diagnostic}. */
export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint';

/** One diagnostic (`bennu_diagnostics`). `start`/`end` are **UTF-8 byte offsets**
 *  into the file source (the editor maps them to CM lint spans). Phase 0 returns []. */
export interface Diagnostic {
  message: string;
  severity: DiagnosticSeverity;
  /** Stable kind slug from the emitting check's `CheckId` (e.g. `"unknown-member"`), for grouping,
   *  suppression or quick-fixes keyed by kind. Empty for diagnostics not yet on the typed catalog. */
  code?: string;
  start: number;
  end: number;
}

// ── build / run (docs §4 "il fondo") ──────────────────────────────────────────

/** A structured build diagnostic parsed from `javac`/`mvn` output (`bennu_build`).
 *  Unlike the editor {@link Diagnostic} (byte offsets over a buffer), a compiler
 *  reports `file:line:col` with no buffer context, so this carries the file + 1-based
 *  line/col. The FE opens `file` and highlights `line:col`. */
export interface BuildDiagnostic {
  /** Offending file (as the compiler emitted it), or `null` when the line had none. */
  file: string | null;
  /** 1-based line, or `null`. */
  line: number | null;
  /** 1-based column, or `null`. */
  col: number | null;
  /** `error` | `warning` | `note`. */
  severity: string;
  message: string;
}

/** Result of `bennu_build`: the parsed diagnostics + which tool ran and whether it
 *  succeeded. The raw log arrives as `arbor://bennu/build-output` events (not inline). */
export interface BuildResult {
  /** The tool that ran: `mvn` or the `javac` fallback. */
  tool: string;
  /** Whether the compile process exited 0. */
  ok: boolean;
  diagnostics: BuildDiagnostic[];
}

/** Per-file timing + counts from a project-wide validation (`bennu_validate_project`). */
export interface FileValidationStat {
  /** Absolute path (forward slashes) of the validated file. */
  file: string;
  /** Milliseconds spent validating it. */
  ms: number;
  /** Number of `error`-severity diagnostics. */
  errors: number;
  /** Number of `warning`-severity diagnostics. */
  warnings: number;
}

/** The diagnostics of one file (byte offsets over its on-disk content). */
export interface FileDiagnostics {
  file: string;
  diagnostics: Diagnostic[];
}

/** Result of `bennu_validate_project` — the whole-project "validation without compiling" with
 *  timing statistics (the compile-time proxy) + diagnostics grouped by file. Aggregates cover every
 *  file; `files` is the slowest-first, capped detail table. */
export interface ProjectValidationResult {
  total_files: number;
  total_ms: number;
  avg_ms: number;
  max_ms: number;
  max_file: string | null;
  total_diagnostics: number;
  error_count: number;
  warning_count: number;
  files: FileValidationStat[];
  diagnostics: FileDiagnostics[];
}

/** Result of `bennu_run`: the id correlating the `arbor://bennu/run-output` /
 *  `arbor://bennu/run-exit` event stream, plus the resolved main class. */
export interface RunHandle {
  run_id: string;
  main_class: string;
}

// ── navigation / tools ─────────────────────────────────────────────────────────

/** One project class (`bennu_class_index`) — powers Go to Class (Ctrl+N). */
export interface ClassEntry {
  /** Dotted fully-qualified class name (`com.acme.Order`). */
  fqcn: string;
  /** Simple name (`Order`). */
  simple: string;
  /** Absolute path (forward slashes) of the declaring file. */
  file: string;
  /** 1-based line of the type declaration. */
  line: number;
  /** Type-kind slug: `class` | `interface` | `enum` | `record` | `annotation` (`''` if unknown) —
   *  drives the file-tree + navigator kind icon. */
  kind: string;
}

/** Index statistics (`bennu_index_stats`) — powers the index inspector. */
export interface IndexStats {
  /** Project types indexed (0 until the first build lands). */
  types: number;
  /** Project members (methods + fields) indexed. */
  members: number;
  /** Resolved JDK language level. */
  jdk_version: string;
  /** Dependency jars on the classpath (not tracked yet → 0). */
  jar_count: number;
  /** Struts actions in the config graph (0 with no web config). */
  actions: number;
  /** Spring beans in the config graph. */
  beans: number;
  /** Config-graph relations (edges). */
  relations: number;
  /** Whether the index/provider has finished building. */
  ready: boolean;
}

/** One match of `bennu_find_in_files` — a single line hit in a project file,
 *  rendered as a row in the Find-in-project modal. */
export interface FindHit {
  /** Absolute path (forward slashes) of the file the match is in. */
  file: string;
  /** 1-based line of the match. */
  line: number;
  /** 1-based column where the match starts (used for the goto + highlight). */
  col: number;
  /** The full source line (trimmed/capped by the BE), for the preview row. */
  preview: string;
}

/** One TODO/FIXME marker (`bennu_todos`) — powers the TODO tool window. */
export interface TodoItem {
  /** Absolute path (forward slashes). */
  file: string;
  /** 1-based line. */
  line: number;
  /** Marker kind: `TODO` | `FIXME` | `XXX` | `HACK`. */
  kind: string;
  /** The comment text after the marker (trimmed, capped). */
  text: string;
}
