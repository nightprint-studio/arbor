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

/** An opened Java project (`bennu_open_project`). */
export interface ProjectInfo {
  /** Absolute project root folder (holds the root `pom.xml`). */
  root: string;
  /** Display name (pom `<name>`, else `<artifactId>`, else the folder name). */
  name: string;
  /** Maven modules (`<modules>`; empty for a single-module project). */
  modules: string[];
  /** Resolved JDK, or `null` when it can't be inferred and no override is set. */
  jdk: JdkInfo | null;
  /** The detected domain capabilities. */
  capabilities: CapabilitySet;
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
}

/** Result of `bennu_write_file`: the encoding the text was actually encoded with
 *  (the project encoding, or `UTF-8` if that couldn't represent a character). */
export interface WriteResult {
  encoding: string;
}

/** One completion candidate (`bennu_completion`). Phase 0 returns `[]`. */
export interface CompletionItem {
  /** Text inserted on accept. */
  label: string;
  /** Kind tag for the icon/grouping (`method`, `field`, `class`, `keyword`, …). */
  kind: string;
  /** Optional signature / type detail shown right of the label. */
  detail?: string;
}

/** Severity of a {@link Diagnostic}. */
export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint';

/** One diagnostic (`bennu_diagnostics`). `start`/`end` are **UTF-8 byte offsets**
 *  into the file source (the editor maps them to CM lint spans). Phase 0 returns []. */
export interface Diagnostic {
  message: string;
  severity: DiagnosticSeverity;
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
