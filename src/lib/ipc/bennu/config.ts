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
  /** Extra JDK install directories to search, on top of `JAVA_HOME` + the standard roots. */
  jdk_paths: string[];
  /** Per-project JDK override, keyed by absolute project-root path → Java version. */
  jdk_overrides: Record<string, string>;
  /** Per-project / per-file encoding override, keyed by absolute path → encoding label. */
  encoding_overrides: Record<string, string>;
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
