/**
 * Bennu (Java & Rust editor) FE types — mirror the BE↔FE contract (`bennu-proto`)
 * field-for-field in **snake_case**. The Rust wire shape is authoritative; see
 * `crates/products/bennu/proto/src/contract.rs`. Types only, no UI/state. The
 * commands live in `$lib/ipc/bennu`.
 */

// The unit-test domains keep their own files: their shapes come from `bennu-test` rather than
// `bennu-proto`, and they carry enough of their own reasoning to be worth reading together. Two
// files because the two build systems genuinely model a test differently — see either header.
export type * from './tests';
export type * from './cargo-tests';

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
  /** The fulcrum engine's i18n convention — an `i18n/languages.toml` with `<lang>/<category>.toml`
   *  bundles beside it. Detected from the layout, so a project that only authors content has it. */
  fulcrum_i18n: boolean;
  /** Bevy ECS — a `bevy` / `bevy_*` dependency in a Cargo manifest, corroborated by the sources. */
  bevy: boolean;
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
  /** Hidden by the platform's convention (a leading `.`, or the Windows attribute).
   *  Omitted on the wire when false — the overwhelmingly common case. */
  hidden?: boolean;
  /** Ignored by git. The tree **marks** these rather than hiding them: a stale ignored
   *  artifact you cannot see is one you cannot explain. Omitted on the wire when false. */
  ignored?: boolean;
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
  /** What the list shows. Also what is inserted when `insert_text` is absent. */
  label: string;
  /** Kind tag for the icon/grouping (`method`, `field`, `class`, `keyword`, …). */
  kind: string;
  /** Optional signature / type detail shown right of the label. */
  detail?: string;
  /** For a type-name completion whose simple name resolves to a SINGLE class: the fully-qualified
   *  name to auto-import on accept (when the auto-import setting is on). Absent for member
   *  completions and ambiguous names. */
  auto_import?: string;

  // ── Fields a language server fills in (absent on the native Java path) ──
  //
  // The distinction between `label` and `insert_text` is load-bearing: `label` is a display
  // string — a server may send `push(…)` or `HashMap (std::collections)` — and inserting it
  // verbatim is how accepting a completion produces code that does not compile.

  /** The text to insert, when it differs from `label`. */
  insert_text?: string;
  /** The byte range this item replaces on accept. Absent → replace the identifier at the caret. */
  replace_start?: number;
  replace_end?: number;
  /** `true` when the provider sent this as a snippet.
   *
   *  Note what it does NOT mean: `insert_text` is plain text either way — the placeholder syntax is
   *  parsed away in the backend, and what is left of it is {@link snippet_stops}. */
  snippet?: boolean;
  /** The snippet's tab stops, as byte ranges into `insert_text`, **in visiting order**.
   *
   *  Visiting order, not source order: the provider's `$0` — where the caret ends up — is already
   *  moved to the end, so a consumer walks the list front to back and needs to know nothing about
   *  the syntax it came from. Empty for a plain completion. */
  snippet_stops?: { start: number; end: number }[];
  /** The provider's own relevance ordering — honoured rather than re-sorted alphabetically. */
  sort_text?: string;
  /** What to match the typed prefix against, when it differs from `label`. */
  filter_text?: string;
  /** Documentation for the info panel (markdown). */
  doc?: string;
  /** Edits elsewhere in the buffer that must land with the insertion — for Rust, the `use` line
   *  an auto-imported item needs. Dropping them produces code that does not compile. */
  edits?: SourceEdit[];
  deprecated?: boolean;
  /** The provider marked this as the one to pre-select. */
  preselect?: boolean;
  /** A handle for fetching this item's documentation lazily (`bennu_lsp_resolve_completion`). */
  resolve_id?: number;
}

/** A plain text edit: replace `[start, end)` of `file` with `new_text`. Byte offsets, applied
 *  through CodeMirror so undo works. */
export interface SourceEdit {
  file: string;
  start: number;
  end: number;
  new_text: string;
}

/**
 * Severity of a {@link Diagnostic}.
 *
 * `weak` sits between "this is wrong" and "this is a note": a **style** finding — true, but not a
 * defect — which is what a naming-convention violation is. It is its own level because a project
 * that adopts a convention gets one finding per offending declaration, and mixing thousands of
 * those in with genuine compile errors would devalue both. CodeMirror has no such level, so the
 * editor maps it onto the softest one it has (see `cmSeverity` in `BennuEditor`); the Problems
 * panel groups it on its own.
 */
export type DiagnosticSeverity = 'error' | 'warning' | 'weak' | 'info' | 'hint';

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
 *  `arbor://bennu/run-exit` event stream, plus what was actually launched. */
export interface RunHandle {
  run_id: string;
  main_class: string;
  /** The spawned command line, quoted for pasting, with the classpath summarised. Comes
   *  from the backend because only it knows which `java` and which resolved classpath. */
  command: string;
  /** The directory the child was started in. */
  working_dir: string;
}

/** One class declaring `public static void main(String[])`, from `bennu_main_classes` —
 *  what the run-config editor offers instead of asking you to type an FQCN. */
export interface MainClassEntry {
  fqcn: string;
  source_file: string | null;
  /** The Maven module it lives in, relative to the root; null on a single-module project. */
  module: string | null;
  /** The declaring type carries `@SpringBootApplication` — this is a Boot entry point. */
  spring_boot: boolean;
}

/** One `key=value` environment row of a persisted run configuration. */
export interface RunConfigEnvVar {
  key: string;
  value: string;
}

/** A persisted run configuration — the wire shape of a `[[bennu.run.configs]]` entry.
 *  snake_case because it crosses the seam; the store's {@link RunConfig} is its camelCase
 *  twin, and `run-config.svelte.ts` owns the one conversion between them. */
export interface RunConfigDto {
  id: string;
  name: string;
  /** `"application"` | `"springboot"` | `"junit"` | `"cargo"` — the category. A string, not a
   *  union, because the wire may carry a kind this build has never heard of (see the Rust
   *  contract). */
  kind: string;
  /** Which part of the project this is for. Maven: the module directory relative to the root.
   *  Cargo: the crate NAME, because that is what `-p` takes. Empty = the root / the workspace. */
  module: string;
  main_class: string;
  program_args: string;
  vm_args: string;
  working_dir: string;
  env: RunConfigEnvVar[];
  /** junit only — `"all"` | `"module"` | `"class"`. */
  test_scope: string;
  /** junit only — the module directory or class selector `test_scope` names. */
  test_target: string;
  /** Hold the VM before `main` when launched under the debugger. */
  debug_suspend?: boolean;
  /** Maven scopes on the run classpath: `"runtime"` (default), `"compile"`, `"test"`, or `""`
   *  for every scope. */
  classpath_scope?: string;
  /** springboot only — the active profiles, comma-separated as Spring spells them. */
  profiles?: string;

  // ── cargo only — the fields of the backend's `Invocation`. Optional: a file written before
  // the Cargo kind existed has none of them, and a hand-edited one may have some.
  cargo_command?: string;
  cargo_target_kind?: string;
  cargo_target?: string;
  cargo_features?: string;
  cargo_all_features?: boolean;
  cargo_no_default_features?: boolean;
  cargo_release?: boolean;
  cargo_profile?: string;
  cargo_workspace?: boolean;
  /** Extra cargo flags, BEFORE the `--`. `program_args` goes after it. */
  cargo_args?: string;
}

/** The per-repo run-config bundle stored in `<root>/.arbor/config.toml`. */
export interface RunConfigSetDto {
  configs: RunConfigDto[];
  active_id: string | null;
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
  /** Whether **the engine that serves this project** can answer. On a Cargo root that is the
   *  language server, not the Java index — which has nothing to build there and so said `false`
   *  for ever. */
  ready: boolean;
  /** What answers questions here: `bennu-index`, or a language server's name and state. Empty on
   *  the editor's own poll, which already knows what it opened; filled for the agent surface,
   *  where the zero counters above would otherwise read as "not built yet". */
  engine?: string;
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
