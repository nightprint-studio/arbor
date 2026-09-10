/**
 * Bennu new-file scaffolding IPC — resolve a new file's path + initial content for the project-tree
 * "New…" menu. Round-trips through the generic `bennu(...)` bridge to `bennu_new_file`; the FE
 * writes the returned content (encoding-aware) and opens it.
 */

import { bennu } from '../rpc';

/** The file kinds the "New…" menu can scaffold.
 *
 *  Two families, because the two languages ask for different things: a Java file is named
 *  by the type it declares, a Rust one names its own module and the types inside are free.
 *  Which family is offered follows the project — a Cargo root has no use for a Java class. */
export type NewFileKind =
  | 'class' | 'interface' | 'enum' | 'record' | 'annotation' | 'exception'
  | 'jsp' | 'xml' | 'file'
  | 'rust_file' | 'rust_struct' | 'rust_enum' | 'rust_trait' | 'rust_module' | 'rust_tests';

/** Whether `kind` is one of the Rust templates. */
export function isRustKind(kind: NewFileKind): boolean {
  return kind.startsWith('rust_');
}

/** Resolved new-file path + content — mirrors the BE `NewFileResult`.
 *
 *  `path` may name a file in a **sub-directory** of the one that was chosen: a Rust module
 *  scaffolds `name/mod.rs`, which is the one kind that creates a directory. */
export interface NewFileResult {
  /** Absolute path (forward slashes) of the file to create. */
  path: string;
  /** Initial content (Java template with inferred package, JSP/XML header, or empty). */
  content: string;
  /** True when a file already exists at `path` (the caller warns instead of overwriting). */
  exists: boolean;
}

/** Scaffold a new file of `kind` named `name` in `dir`. Resolves the path + content (package
 *  inferred from `dir` for Java kinds); `null` for an unknown kind. Wire: `bennu_new_file`. */
export function newFile(dir: string, name: string, kind: NewFileKind): Promise<NewFileResult | null> {
  return bennu('bennu_new_file', { args: { dir, name, kind } });
}

// ── new module (Maven) ───────────────────────────────────────────────────────────

/** One pom a new module could be added under — a row of the New Module dialog's parent picker. */
export interface ModuleParent {
  /** Absolute path of the pom, forward-slashed. */
  pom: string;
  /** Absolute path of its directory — where the new module's folder goes. */
  dir: string;
  /** Project-relative directory; empty for the root. */
  relative: string;
  /** What a child would inherit. */
  group_id: string;
  artifact_id: string;
  version: string;
  packaging: string;
  /** Already an aggregator — adding a module changes nothing about it. */
  aggregator: boolean;
  /** Set when it cannot become one without disabling what it builds; the reason to show. */
  blocked: string | null;
}

/** The poms a new module could be added under, outermost first.
 *
 *  Follows the reactor's `<modules>` rather than the directory tree: a `samples/` folder with a
 *  pom of its own is not part of this build, and a module added under it would compile for nobody.
 *
 *  Wire: `bennu_new_module_context` — `RootArgs { root }`. */
export function newModuleContext(root: string): Promise<{ parents: ModuleParent[] }> {
  return bennu('bennu_new_module_context', { args: { root } });
}

/** What creating a module comes to. */
export interface NewModuleResult {
  module_dir: string;
  /** The new pom — what the caller opens. */
  pom: string;
  /** Every path created, so the tree can be told what appeared. */
  created: string[];
  /** Whether the parent's packaging had to become `pom`. */
  parent_became_aggregator: boolean;
}

/** Create a Maven module: its folder, its pom, its source roots, and its entry in the parent's
 *  `<modules>` — the last of which is what separates a module from a directory that looks like one.
 *
 *  `group_id` and `version` are written into the new pom **only** when they differ from what the
 *  parent gives; empty means inherit, which is what nearly every module should do.
 *
 *  Rejects — before writing anything — on a name that is not a folder name, a directory that
 *  already has something in it, and a parent that builds sources of its own.
 *
 *  Wire: `bennu_new_module` — `NewModuleArgs { root, parent_pom, artifact_id, dir_name, group_id,
 *  version, packaging, name }`. */
export function newModule(spec: {
  root: string;
  parent_pom: string;
  artifact_id: string;
  dir_name?: string;
  group_id?: string;
  version?: string;
  packaging?: string;
  name?: string;
}): Promise<NewModuleResult> {
  return bennu('bennu_new_module', { args: spec });
}

// ── copy / paste in the project tree ─────────────────────────────────────────────

/** One file a paste would create. */
export interface PasteItem {
  /** The file being copied. */
  source: string;
  /** The name it would take in the target — what the dialog's name field starts from. */
  name: string;
  /** Where it would land. */
  target: string;
  /** A Java type, so its package (and, on a rename, its type) will be rewritten. */
  java: boolean;
  /** The type the file declares, when it declares one. */
  type_name: string;
  /** The package it would declare after the paste; empty outside a source root. */
  package: string;
  /** Something is already there — the caller must offer another name. */
  collides: boolean;
  /** Why this one cannot be pasted at all, when it cannot. */
  refused: string | null;
}

/** Where each copied file would land, and what is in the way. Writes nothing.
 *
 *  Wire: `bennu_paste_plan` — `PastePlanArgs { root, sources, target_dir }`. */
export function pastePlan(
  root: string,
  sources: string[],
  targetDir: string,
): Promise<{ items: PasteItem[] }> {
  return bennu('bennu_paste_plan', { args: { root, sources, target_dir: targetDir } });
}

/** What a paste wrote. */
export interface PasteResult {
  /** The files created; the first is the one to open. */
  written: string[];
  /** The imports that had to be added for the copy to still resolve, fully qualified. */
  imports_added: string[];
}

/** Copy the files into `targetDir`. A Java file's `package` is rewritten to where it lands, its
 *  type renamed when `newName` renames the file, and the neighbours it referred to by simple name
 *  become imports when the copy leaves their package behind.
 *
 *  `newName` applies only to a single file. Rejects rather than overwriting anything.
 *
 *  Wire: `bennu_paste_files` — `PasteArgs { root, sources, target_dir, new_name }`. */
export function pasteFiles(
  root: string,
  sources: string[],
  targetDir: string,
  newName = '',
): Promise<PasteResult> {
  return bennu('bennu_paste_files', {
    args: { root, sources, target_dir: targetDir, new_name: newName },
  });
}
