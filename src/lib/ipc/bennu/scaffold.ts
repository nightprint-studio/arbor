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
