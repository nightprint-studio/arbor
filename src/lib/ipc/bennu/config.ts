/**
 * Bennu product-config IPC — the typed per-profile `…/bennu/config.toml` the editor
 * persists (default encoding, indent width, extra JDK search paths, per-project JDK /
 * encoding overrides). Round-trips through the generic `bennu(...)` rpc bridge to the
 * `get_bennu_config` / `set_bennu_config` handlers in `bennu-be`.
 *
 * Kept in its own file so concurrent edits to the main bennu IPC surface don't race.
 */

import { bennu } from '../rpc';

/** Mirrors the BE `BennuConfig` (snake_case, field-for-field). */
export interface BennuConfig {
  /** Fallback text encoding when a project declares none. */
  default_encoding: string;
  /** Editor indentation width in spaces. */
  indent_width: number;
  /** Which SQL dialect `.sql` buffers are highlighted as — `'oracle'` / `'postgres'` /
   *  `'portable'` (default). Highlighting only: a `.sql` file in a Java project carries nothing
   *  that says which engine it targets, and the two disagree about string quoting. */
  sql_dialect: string;
  /** The build the split-button runs by default (and on Ctrl+F9): `'mvn'` (Maven compile) or
   *  `'validate'` (whole-project validation without compiling). */
  preferred_build_type: string;
  /** Warm up the whole-project validation cache in the background right after indexing, so the
   *  first "Validate (no compile)" is instant. `true` by default; off avoids the background CPU. */
  validate_on_open: boolean;
  /** Autosave a modified buffer to disk automatically (after a short idle, on tab switch, and on
   *  window blur). `true` by default; off saves only on Ctrl+S. */
  autosave: boolean;
  /** Fold runs of library frames in the debugger's call stack into one expandable row. */
  collapse_library_frames: boolean;
  /** Offer the classes and files inside the dependency jars in the Go-to navigator, as two extra
   *  categories searched in the backend. `false` by default. */
  search_dependencies: boolean;
  /** Class-name patterns a debugger step passes straight through (`java.*`,
   *  `org.springframework.*`). Empty = the backend defaults. A `*` is allowed at one end
   *  only; anything else is dropped rather than sent to the VM. */
  step_excludes: string[];
  /** Auto-import on accepting a type-name completion whose simple name resolves to a SINGLE class.
   *  `true` by default; off inserts just the name (import later with Alt+Enter). */
  auto_import: boolean;
  /** Max worker threads the whole-project validation sweep may use. `0` = auto (leaves ~half the
   *  cores free for the UI / go-to); set a small number (e.g. `1`) so a big project's validation
   *  can't peg every core and freeze the editor. Doesn't affect the initial index build. */
  validation_threads: number;
  /** Extra JDK install directories to search, on top of `JAVA_HOME` + the standard roots. */
  jdk_paths: string[];
  /** Per-project JDK override, keyed by absolute project-root path → Java version. */
  jdk_overrides: Record<string, string>;
  /** Per-project / per-file encoding override, keyed by absolute path → encoding label. */
  encoding_overrides: Record<string, string>;
  /** Which dependencies contribute their Spring beans to the Library beans view. Empty by
   *  default, and empty means no jar is ever opened. */
  library_beans: LibraryBeansConfig;
  /** Language servers — which may run, where their binaries are, and any the user added. */
  lsp: LspConfigDto;
  /** Cargo / crates.io — the one part of Bennu that reaches the network on its own. */
  cargo: CargoConfigDto;
}

/** Mirrors the BE `CargoConfig`. */
export interface CargoConfigDto {
  /** Whether Bennu may query the crates.io index — the version hints and the add dialog's version
   *  list. `true` by default; off makes Bennu entirely local again. */
  crates_io: boolean;
  /** How long a cached version list stays fresh, in hours. A day by default; `0` reads as the
   *  default rather than as "always refetch". */
  index_ttl_hours: number;
}

/** Mirrors the BE `LspConfig`. */
export interface LspConfigDto {
  /** Master switch. `true` by default — a server only starts for a project whose root carries
   *  the matching manifest AND whose binary is installed, so "on" costs nothing on a machine
   *  with nothing installed. */
  enabled: boolean;
  /** What rust-analyzer runs to produce **real** diagnostics on save: `check` or `clippy`.
   *
   *  `check` by default — it is what `cargo build` would have told you, and it is the faster of the
   *  two. `clippy` is a superset (every check error plus several hundred lints) at the cost of a
   *  slower build after every save. */
  rust_check_command: string;
  /** Server ids the user turned off. A denylist, so a server added to the catalogue later works
   *  without the user editing anything. */
  disabled: string[];
  /** Explicit executable path per server id, for a binary discovery does not find (or a specific
   *  build the user wants). An absolute path wins over everything. */
  server_paths: Record<string, string>;
  /** User-defined servers, for a language the catalogue does not cover. */
  servers: CustomLspServerDto[];
  /** How long a server with **no window showing its project** may sit idle before it is stopped,
   *  in seconds. `0` never stops one.
   *
   *  Such a session exists because something asked about a project nobody has open — an AI client,
   *  in practice. rust-analyzer is most of a gigabyte resident, and nothing else reclaims one. A
   *  server a window opened is never stopped by this. */
  background_idle_timeout_secs: number;
}

/** Mirrors the BE `CustomLspServer` — the same fields the built-in catalogue carries, because the
 *  two are interchangeable by design. An entry whose `id` matches a built-in replaces it. */
export interface CustomLspServerDto {
  id: string;
  name: string;
  /** The LSP `languageId` (`'zig'`). Falls back to `id`. */
  language: string;
  command: string;
  args: string[];
  /** File extensions without dots. Required — an entry serving none can never be selected. */
  extensions: string[];
  /** Files marking a workspace root (`['build.zig']`). Required, and the real gate: without a
   *  marker above the file there is no workspace to open, so nothing starts. */
  root_markers: string[];
  /** `initializationOptions` as a JSON string. A string because these are arbitrary
   *  server-defined JSON and TOML cannot express all of it losslessly. */
  initialization_options: string;
}

/** Which dependency coordinates are read for their beans. Four axes because that is how a
 *  coordinate gets matched in practice: one artifact, a whole group, everything an
 *  organisation publishes (`com.acme.` — the trailing dot matters), or a naming convention. */
export interface LibraryBeansConfig {
  group_id: string[];
  group_id_prefix: string[];
  artifact_id: string[];
  artifact_id_prefix: string[];
}

/** Read the typed bennu config (BE returns defaults on a missing/corrupt file). */
export function getBennuConfig(): Promise<BennuConfig> {
  return bennu('get_bennu_config', {});
}

/** Persist the typed bennu config. The BE also re-seeds the classpath's extra JDK search
 *  dirs from `jdk_paths` so a change takes effect on the next index build. */
export function setBennuConfig(config: BennuConfig): Promise<void> {
  return bennu('set_bennu_config', { config });
}

/** One project's editor session inside a workspace — mirrors the BE `ProjectSession`. */
export interface ProjectSession {
  /** Absolute (forward-slashed) project root — the session key. */
  root: string;
  /** The open editor tabs (file paths) in tab order — may include files opened from OTHER
   *  workspace projects (the FE flags those as foreign). */
  open_files: string[];
  /** The active tab (one of `open_files`), or ''. */
  active_file: string;
}

/** Mirrors the BE `BennuWorkspace` — one named workspace: an ordered set of Java projects with
 *  per-project sessions. Switching workspace reopens a whole different set of projects. The same
 *  project may belong to several workspaces (each keeps its own tabs). */
export interface BennuWorkspace {
  /** Stable id (FE-generated uuid). */
  id: string;
  /** Display name ('' for the implicit default workspace). */
  name: string;
  /** Palette index (0..11) for the workspace monogram. */
  color_idx: number;
  /** Root of the active project (one of `projects[].root`), or ''. */
  active_project: string;
  /** The member projects + their sessions, in switch order. */
  projects: ProjectSession[];
}

/** Mirrors the BE `BennuWorkspaces` — the whole store (its own `workspace.toml`, separate from the
 *  stable settings): every named workspace plus which is active. A pre-named-workspaces file is
 *  migrated to a single default-named workspace on read. */
export interface BennuWorkspaces {
  /** Id of the active workspace (one of `workspaces[].id`), or ''. */
  active_id: string;
  /** Every workspace, in display order. */
  workspaces: BennuWorkspace[];
}

/** Read the persisted workspace store (empty store on a missing/corrupt file). */
export function getBennuWorkspaces(): Promise<BennuWorkspaces> {
  return bennu('get_bennu_workspaces', {});
}

/** Persist the workspace store — call debounced on tab/project/switch/CRUD changes. */
export function setBennuWorkspaces(workspaces: BennuWorkspaces): Promise<void> {
  return bennu('set_bennu_workspaces', { workspaces });
}
