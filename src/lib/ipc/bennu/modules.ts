/**
 * Project modules IPC — every build manifest of a project, not just the one that decides its kind.
 *
 * A project is often more than one thing: a Tauri app is a Node project with a Cargo project in
 * `src-tauri`, a monorepo holds a Maven backend beside an npm frontend. `bennu_open_project` still
 * names ONE primary kind (it drives the Java index); this is the complementary question — "which
 * build tools does this folder carry, and where" — which the activity bar and the tool windows ask.
 *
 * Members of a workspace (Cargo crates, Maven modules, npm workspace packages) are NOT listed
 * beside their parent: the ecosystem's own tool window already shows them.
 *
 * Mirrors `bennu_project::Module`. Same convention as the rest of `ipc/bennu`: fields under
 * `{ args: … }`.
 */

import { bennu } from '../rpc';

/** The build ecosystems the backend recognises. */
export type ModuleKind = 'Maven' | 'Gradle' | 'Cargo' | 'Node';

/** One build manifest of a project. */
export interface ProjectModule {
  kind: ModuleKind;
  /** The module's folder relative to the project root, forward slashes; `.` for the root itself. */
  dir: string;
}

/** Every build manifest under `root`. Cheap — a few directory listings. Wire: `bennu_project_modules`. */
export function projectModules(root: string): Promise<ProjectModule[]> {
  return bennu('bennu_project_modules', { args: { root } });
}
