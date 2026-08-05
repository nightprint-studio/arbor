/**
 * Bennu (Java editor) IPC — thin `bennu(...)` rpc wrappers over the Model-D bridge.
 *
 * Types only + wrappers — no UI, no state. Every command routes through the generic
 * `rpc` bridge to **`bennu-be`** via the bound {@link bennu} helper: `bennu('<handler>',
 * params)`, where `<handler>` is the exact backend handler name (snake_case = the Rust
 * fn name).
 *
 * ⚠️ Arg convention: the RPC seam keys params by the handler's **parameter name**, not
 * by the struct field. Every `bennu-be` handler takes a single struct parameter named
 * `args`, so each call wraps its fields under `{ args: … }` (the proven tyto/studio
 * convention) — NOT a bare/flat object. The inner field names are the struct's fields
 * in snake_case (forwarded verbatim inside the opaque `params`).
 *
 * TS function names are camelCase; wire method names are the exact snake_case strings.
 */

import { bennu } from '../rpc';
import type {
  ProjectInfo, TreeNode, ReadFileResult, CapabilitySet, CompletionItem, Diagnostic,
  BuildResult, ProjectValidationResult, RunHandle, WriteResult, ClassEntry, TodoItem, IndexStats,
  FileDiagnostics, FileStamp, MainClassEntry, RunConfigSetDto,
} from '$lib/types/bennu';

/** Open a Java project folder: resolve the build model (modules / JDK) + capabilities.
 *  Wire: `bennu_open_project` — `OpenProjectArgs { root }`. */
export function openProject(root: string): Promise<ProjectInfo> {
  return bennu('bennu_open_project', { args: { root } });
}

/** Read the project file tree (directories + files) rooted at `root`. Wire:
 *  `bennu_project_tree` — `ProjectTreeArgs { root, depth? }`. */
export function projectTree(root: string): Promise<TreeNode> {
  return bennu('bennu_project_tree', { args: { root } });
}

/** Read a file's text + the encoding it was decoded from. `root` (the project root)
 *  is needed so the backend can resolve the pom-declared encoding. Wire:
 *  `bennu_read_file` — `ReadFileArgs { root, file }`. */
export function readFile(root: string, file: string): Promise<ReadFileResult> {
  return bennu('bennu_read_file', { args: { root, file } });
}

/**
 * Write `text` to `file` on disk, encoded with the project's resolved encoding
 * (round-trips `bennu_read_file`; falls back to UTF-8 when a char can't be encoded).
 * Returns the encoding actually used + the file's new stamp.
 *
 * `expectStamp` is the overwrite guard: pass the stamp the buffer was read from and the
 * write is **refused** — with an {@link ERR_EXTERNALLY_MODIFIED}-prefixed error — when the
 * file changed underneath. Omit it only for a file that was never read (a fresh one).
 *
 * Wire: `bennu_write_file` — `WriteFileArgs { root, file, text, expect_stamp? }`.
 */
export function writeFile(
  root: string,
  file: string,
  text: string,
  expectStamp?: string,
): Promise<WriteResult> {
  return bennu('bennu_write_file', { args: { root, file, text, expect_stamp: expectStamp ?? null } });
}

/** The prefix of the `bennu_write_file` error that means "the file changed on disk since
 *  you read it". Mirrors `bennu_proto::ERR_EXTERNALLY_MODIFIED` — the error string is the
 *  contract across the seam, so this is the one message a caller matches on. */
export const ERR_EXTERNALLY_MODIFIED = 'bennu:externally-modified';

/** True when `err` is the write-refused-because-the-file-changed error. */
export function isExternallyModifiedError(err: unknown): boolean {
  return String(err).includes(ERR_EXTERNALLY_MODIFIED);
}

/** Stat `files` and return each one's current on-disk stamp — the external-change poll for
 *  the open tabs. Never rejects for a missing path (that comes back `exists: false`).
 *  Wire: `bennu_file_stamps` — `FileStampsArgs { files }`. */
export function fileStamps(files: string[]): Promise<FileStamp[]> {
  return bennu('bennu_file_stamps', { args: { files } });
}

/** Move a `.java` file into the folder matching the `package` it declares (the filesystem
 *  alternative to the change-package edit). Returns the new absolute path. Save the buffer first —
 *  this renames the on-disk file. Wire: `bennu_move_to_package` — `{ file, source }`. */
export function moveToPackage(file: string, source: string): Promise<{ new_path: string }> {
  return bennu('bennu_move_to_package', { args: { file, source } });
}

/** Re-detect the domain capabilities (Spike-D bitset) for the open project. Wire:
 *  `bennu_capabilities` — `CapabilitiesArgs { root }`. */
export function capabilities(root: string): Promise<CapabilitySet> {
  return bennu('bennu_capabilities', { args: { root } });
}

/** Completion candidates at a source offset (UTF-8 byte offset). Pass the live buffer `source`: the
 *  `offset` is in its coordinates and the just-typed `.` that triggers member completion is unsaved,
 *  so the backend must parse this text, not the stale on-disk file. Wire: `bennu_completion` —
 *  `CompletionArgs { file, offset, source }`. Returns `[]` until the language service is ready. */
export function completion(file: string, offset: number, source: string): Promise<CompletionItem[]> {
  return bennu('bennu_completion', { args: { file, offset, source } });
}

/** A ready import edit (byte range + replacement) for auto-import on completion accept. */
export interface ImportEdit {
  start: number;
  end: number;
  replacement: string;
}

/** Compute the `import <fqn>;` edit for `source`, or `null` when no import is needed (java.lang,
 *  same package, already imported, star-covered). Called on accepting a type-name completion whose
 *  `auto_import` is set and the auto-import setting is on. Wire: `bennu_import_edit`. */
export function importEdit(source: string, fqn: string): Promise<ImportEdit | null> {
  return bennu('bennu_import_edit', { args: { source, fqn } });
}

/** Diagnostics for a file. For a Java file, pass the live buffer `source` to get AST-level
 *  validation (syntax errors + unused imports) without compiling; for a JSP the backend checks
 *  action/include references itself. `resolved` picks the Java validation tier — `false` = the fast
 *  pure-AST pass only (syntax / structure — instant while typing), `true` (default) = the full
 *  resolver-backed pass; the FE runs both on staggered debounces so a big file stays responsive.
 *  Wire: `bennu_diagnostics` — `DiagnosticsArgs { file, source?, resolved? }`. */
export function diagnostics(file: string, source?: string, resolved = true): Promise<Diagnostic[]> {
  return bennu('bennu_diagnostics', { args: { file, source, resolved } });
}

/** Compile the project: `mvn -q -o compile` (offline, project JDK) with a `javac`
 *  fallback. With `module`, only that module and the ones it is built from (`-pl … -am`) —
 *  what the launch path passes, since a run only needs its own module's output. The raw log
 *  streams as `arbor://bennu/build-output`; the resolved promise carries the parsed
 *  diagnostics. A clean build re-indexes `target/classes`. Wire: `bennu_build` —
 *  `BuildArgs { root, module? }`. */
export function build(root: string, module = ''): Promise<BuildResult> {
  return bennu('bennu_build', { args: { root, module } });
}

/** Validate the WHOLE project without compiling: walk every `.java`, run the editor's per-file
 *  validation over all of them, and return timing stats (total / average / slowest file) + the
 *  diagnostics grouped by file. Progress streams as `arbor://bennu/validate-progress`, ending with
 *  `arbor://bennu/validate-done`. Shares the Maven build's single-run guard (one at a time). Wire:
 *  `bennu_validate_project` — `ValidateProjectArgs { root }`. */
export function validateProject(root: string): Promise<ProjectValidationResult> {
  return bennu('bennu_validate_project', { args: { root } });
}

/** Cancel the running whole-project validation. Fire-and-forget: the BE stops the sweep and discards
 *  its partial results (no cache written). No-op if nothing is validating. Wire:
 *  `bennu_cancel_validation` — `ValidateProjectArgs { root }`. */
export function cancelValidation(root: string): Promise<void> {
  return bennu('bennu_cancel_validation', { args: { root } });
}

/** SILENT whole-project re-validation for the live Problems panel (the on-save refresh): no build
 *  guard, no progress events, no stats — just the diagnostics grouped by file, cheap thanks to the
 *  incremental cache. `null` when the project's index isn't ready yet (leave the panel as-is). Wire:
 *  `bennu_project_diagnostics` — `ValidateProjectArgs { root }`. */
export function projectDiagnostics(root: string): Promise<FileDiagnostics[] | null> {
  return bennu('bennu_project_diagnostics', { args: { root } });
}

/** Everything a launch can carry beyond the main class. All optional — a caller with only
 *  a class still runs. */
export interface RunOptions {
  /** The Maven module the class lives in, relative to the root. Decides the classpath —
   *  omitting it on a multi-module project runs against the reactor root, which compiles
   *  nothing. */
  module?: string;
  /** Program arguments, after the main class. */
  args?: string[];
  /** JVM options (`-Xmx…`, `-D…`), before `-cp`. */
  vmArgs?: string[];
  /** Working directory; empty/omitted = the project root. */
  workingDir?: string;
  /** Extra environment, merged over the inherited one. */
  env?: Record<string, string>;
  /** Launch under the debugger: the JVM gets the JDWP agent, connecting back to a port the
   *  backend opens first, and the debug session carries this run's id. */
  debug?: boolean;
  /** Hold the VM before `main` until the debugger has installed everything. Off unless the
   *  run configuration asked for it — it is the only way to stop in start-up code, and it
   *  means the launch begins frozen. */
  debugSuspend?: boolean;
}

/** Launch `java <vm…> -cp <target/classes:deps> <mainClass> <args…>`, streaming stdout/stderr
 *  as `arbor://bennu/run-output` and ending with `arbor://bennu/run-exit`. Resolves
 *  immediately with the run handle (which carries the command line that was actually spawned).
 *  Wire: `bennu_run` — `RunArgs { root, main_class, args?, vm_args?, working_dir?, env? }`. */
export function run(root: string, mainClass: string, opts: RunOptions = {}): Promise<RunHandle> {
  return bennu('bennu_run', {
    args: {
      root,
      main_class: mainClass,
      module: opts.module ?? '',
      args: opts.args ?? [],
      vm_args: opts.vmArgs ?? [],
      working_dir: opts.workingDir ?? '',
      env: opts.env ?? {},
      debug: opts.debug ?? false,
      debug_suspend: opts.debugSuspend ?? false,
    },
  });
}

/** Feed one line to a live run's stdin (the console's input box). The newline is added by
 *  the backend. Rejects when the run has already exited. Wire: `bennu_run_input` —
 *  `RunInputArgs { run_id, text }`. */
export function runInput(runId: string, text: string): Promise<void> {
  return bennu('bennu_run_input', { args: { run_id: runId, text } });
}

/** Stop a live run — the process TREE, not just the handle we hold. Resolves `true` if a run
 *  was killed. Wire: `bennu_cancel_run` — `CancelRunArgs { run_id }`. */
export function cancelRun(runId: string): Promise<boolean> {
  return bennu('bennu_cancel_run', { args: { run_id: runId } });
}

/** Every class in the project declaring `public static void main(String[])` — a source scan
 *  (so it works before the index is built), pre-filtered and cached per project on the
 *  backend, dropped when the project is re-indexed. Prefer `bennuMainClassStore`, which
 *  shares one read between the picker, ▷ and the Spring Boot resolution. Wire:
 *  `bennu_main_classes` — `MainClassesArgs { root, force? }`. */
export function mainClasses(root: string, force = false): Promise<MainClassEntry[]> {
  return bennu('bennu_main_classes', { args: { root, force } });
}

/** Read the per-repo run configurations from `<root>/.arbor/config.toml` `[bennu.run]`. A repo
 *  that has never had one yields an empty bundle. Wire: `bennu_get_run_config`. */
export function getRunConfig(root: string): Promise<RunConfigSetDto> {
  return bennu('bennu_get_run_config', { args: { root } });
}

/** Persist the per-repo run configurations, leaving every other section of the shared
 *  `.arbor/config.toml` untouched. Wire: `bennu_set_run_config`. */
export function setRunConfig(root: string, configSet: RunConfigSetDto): Promise<void> {
  return bennu('bennu_set_run_config', { args: { root, config_set: configSet } });
}

/** List every project class (a fresh source scan) for the Go-to-Class navigator.
 *  Wire: `bennu_class_index` — `ClassIndexArgs { root }`. */
export function classIndex(root: string): Promise<ClassEntry[]> {
  return bennu('bennu_class_index', { args: { root } });
}

/** Scan the project for TODO/FIXME/XXX/HACK markers (the TODO tool window). Wire:
 *  `bennu_todos` — `TodoScanArgs { root }`. */
export function todos(root: string): Promise<TodoItem[]> {
  return bennu('bennu_todos', { args: { root } });
}

/** Index statistics for the open project (the index inspector). Pass the EXACT opened
 *  root (matched by equality, not prefix). Wire: `bennu_index_stats` —
 *  `IndexStatsArgs { root }`. */
export function indexStats(root: string): Promise<IndexStats> {
  return bennu('bennu_index_stats', { args: { root } });
}

/** Start a **progressive** project-wide search for `query`. Fire-and-forget: results
 *  stream back as `arbor://bennu/find-progress` events tagged with `searchId`
 *  (`{ id, hits?: FindHit[], done?: boolean, capped?: boolean }`) as the scan walks the
 *  tree, so a large legacy project fills the results list incrementally instead of
 *  blocking until the end. `regex` treats `query` as a pattern; `caseSensitive` /
 *  `wholeWord` refine plain and regex matches alike. The caller owns `searchId` (a fresh
 *  one per search) and ignores events from superseded ids. Resolves once the scan has
 *  been scheduled (not when it finishes — the terminal `done` event signals that).
 *  Wire: `bennu_find_in_files` — `FindInFilesArgs { root, query, regex, case_sensitive,
 *  whole_word, search_id }`. */
export function findInFiles(
  root: string,
  query: string,
  opts: {
    regex: boolean;
    caseSensitive: boolean;
    wholeWord: boolean;
    extraRoots?: string[];
    /** Also search the text entries of the project's dependency jars. Off by default: every
     *  candidate entry has to be decompressed to be read, so it is a different order of cost
     *  from walking the tree. Those hits arrive last, and their `file` is `<jar>!/<entry>`. */
    includeDependencies?: boolean;
  },
  searchId: string,
): Promise<void> {
  return bennu('bennu_find_in_files', {
    args: {
      root,
      extra_roots: opts.extraRoots ?? [],
      query,
      regex: opts.regex,
      case_sensitive: opts.caseSensitive,
      whole_word: opts.wholeWord,
      include_dependencies: opts.includeDependencies ?? false,
      search_id: searchId,
    },
  });
}
