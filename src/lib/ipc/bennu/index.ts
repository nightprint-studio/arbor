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
  FileDiagnostics,
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

/** Write `text` to `file` on disk, encoded with the project's resolved encoding
 *  (round-trips `bennu_read_file`; falls back to UTF-8 when a char can't be encoded).
 *  Returns the encoding actually used. Wire: `bennu_write_file` —
 *  `WriteFileArgs { root, file, text }`. */
export function writeFile(root: string, file: string, text: string): Promise<WriteResult> {
  return bennu('bennu_write_file', { args: { root, file, text } });
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

/** Completion candidates at a source offset (UTF-8 byte offset). Wire:
 *  `bennu_completion` — `CompletionArgs { file, offset }`. Returns `[]` until the
 *  language service is ready. */
export function completion(file: string, offset: number): Promise<CompletionItem[]> {
  return bennu('bennu_completion', { args: { file, offset } });
}

/** Diagnostics for a file. For a Java file, pass the live buffer `source` to get AST-level
 *  validation (syntax errors + unused imports) without compiling; for a JSP the backend checks
 *  action/include references itself. Wire: `bennu_diagnostics` — `DiagnosticsArgs { file, source? }`. */
export function diagnostics(file: string, source?: string): Promise<Diagnostic[]> {
  return bennu('bennu_diagnostics', { args: { file, source } });
}

/** Compile the project: `mvn -q -o compile` (offline, project JDK) with a `javac`
 *  fallback. The raw log streams as `arbor://bennu/build-output`; the resolved promise
 *  carries the parsed diagnostics. A clean build re-indexes `target/classes`. Wire:
 *  `bennu_build` — `BuildArgs { root }`. */
export function build(root: string): Promise<BuildResult> {
  return bennu('bennu_build', { args: { root } });
}

/** Validate the WHOLE project without compiling: walk every `.java`, run the editor's per-file
 *  validation over all of them, and return timing stats (total / average / slowest file) + the
 *  diagnostics grouped by file. Progress streams as `arbor://bennu/validate-progress`, ending with
 *  `arbor://bennu/validate-done`. Shares the Maven build's single-run guard (one at a time). Wire:
 *  `bennu_validate_project` — `ValidateProjectArgs { root }`. */
export function validateProject(root: string): Promise<ProjectValidationResult> {
  return bennu('bennu_validate_project', { args: { root } });
}

/** SILENT whole-project re-validation for the live Problems panel (the on-save refresh): no build
 *  guard, no progress events, no stats — just the diagnostics grouped by file, cheap thanks to the
 *  incremental cache. `null` when the project's index isn't ready yet (leave the panel as-is). Wire:
 *  `bennu_project_diagnostics` — `ValidateProjectArgs { root }`. */
export function projectDiagnostics(root: string): Promise<FileDiagnostics[] | null> {
  return bennu('bennu_project_diagnostics', { args: { root } });
}

/** Launch `java -cp <target/classes:deps> <mainClass> <args…>`, streaming stdout/stderr
 *  as `arbor://bennu/run-output` and ending with `arbor://bennu/run-exit`. Resolves
 *  immediately with the run handle. `mainClass` is required (main-class discovery is a
 *  later wave). Wire: `bennu_run` — `RunArgs { root, main_class, args? }`. */
export function run(root: string, mainClass: string, args: string[] = []): Promise<RunHandle> {
  return bennu('bennu_run', { args: { root, main_class: mainClass, args } });
}

/** Stop a running `bennu_run` child by id. Resolves `true` if a live run was stopped.
 *  Wire: `bennu_cancel_run` — `CancelRunArgs { run_id }`. */
export function cancelRun(runId: string): Promise<boolean> {
  return bennu('bennu_cancel_run', { args: { run_id: runId } });
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
  opts: { regex: boolean; caseSensitive: boolean; wholeWord: boolean; extraRoots?: string[] },
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
      search_id: searchId,
    },
  });
}
