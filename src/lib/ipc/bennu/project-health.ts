/**
 * Bennu project-health IPC — the states in which a project stops being what was opened: a root that
 * vanished, reactor modules the disk lost, a file no indexed project covers.
 *
 * Wire: `bennu_file_owner`, `bennu_root_status` (see `bennu-be/src/project_health.rs`), plus the two
 * events below.
 */

import { bennu } from '../rpc';

/** An open project's root directory is gone; the backend has already released its index. */
export const PROJECT_MISSING = 'arbor://bennu/project-missing';

/** A reactor declares modules the disk does not have. Emitted once per episode. */
export const MODULES_MISSING = 'arbor://bennu/modules-missing';

/** Payload of {@link PROJECT_MISSING}. */
export interface ProjectMissing {
  root: string;
  /** The nearest ancestor that still exists — where a folder picker should start. */
  nearest_existing: string | null;
}

/** One `<module>` a pom lists that has no `pom.xml` on disk. */
export interface MissingModuleEntry {
  name: string;
  /** The pom whose `<modules>` lists it. */
  pom: string;
}

/** Payload of {@link MODULES_MISSING}. */
export interface ModulesMissing {
  root: string;
  modules: MissingModuleEntry[];
  /** Pom-bearing directories beside the declaring pom that no `<modules>` lists — usually the
   *  renamed directory itself. */
  unlisted: string[];
}

/** Which indexed project owns a file. */
export interface FileOwnership {
  /** `null` when no indexed project contains the file. A project still building is an owner. */
  owner: { root: string; ready: boolean } | null;
  /** The reactor root enclosing the file, offered only when there is no owner. */
  suggested_root: string | null;
}

/** Whether a remembered project root is still there. */
export interface RootStatus {
  exists: boolean;
  nearest_existing: string | null;
}

/** Wire: `bennu_file_owner` — `{ file }`. */
export function fileOwner(file: string): Promise<FileOwnership> {
  return bennu('bennu_file_owner', { args: { file } });
}

/** Wire: `bennu_root_status` — `{ root }`. */
export function rootStatus(root: string): Promise<RootStatus> {
  return bennu('bennu_root_status', { args: { root } });
}
