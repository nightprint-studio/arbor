/**
 * Bennu dependencies IPC — what the project depends on, and who decided each answer.
 *
 * Mirrors `bennu-deps`'s model field-for-field in **snake_case**; the Rust side
 * (`crates/products/bennu/deps/src/model.rs`) is authoritative.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC surface don't
 * race — same reasoning as `ext.ts`.
 */

import { bennu } from '../rpc';

/** Where a dependency's presence — or its version — was decided.
 *
 *  Three genuinely different facts, which is why this is a union and not a boolean:
 *  `declared` needs no explanation, `managed` means the module asked for the artifact and a
 *  `<dependencyManagement>` somewhere up the chain chose the version, and `inherited` means the
 *  module never mentioned it at all — a parent pom's own `<dependencies>` are every child's. */
export type DependencyOrigin =
  | { kind: 'declared' }
  | { kind: 'managed'; from: string }
  | { kind: 'inherited'; from: string };

/** A place in a file, for go-to. */
export interface DependencySite {
  /** Absolute path, forward-slashed. */
  file: string;
  /** Byte offset of the `<dependency>` tag. */
  offset: number;
  /** 1-based line. */
  line: number;
}

/** One dependency of one module, everything resolved that could be. */
export interface Dependency {
  group_id: string;
  artifact_id: string;
  /** `${…}` expanded and `<dependencyManagement>` applied. **Empty when nothing on disk answers
   *  it** — an imported BOM, a parent that only exists in the repository. Never guessed. */
  version: string;
  /** `compile` when the pom doesn't say — Maven's own default, made explicit. */
  scope: string;
  /** `<type>` when it isn't the default `jar`. */
  packaging: string;
  classifier: string;
  optional: boolean;
  origin: DependencyOrigin;
  /** The profile whose `<dependencies>` this came from, empty for the ordinary case. */
  profile: string;
  /** The pom that declares it, and where — so the row is somewhere to go. */
  declared_in: DependencySite;
  /** The jar in the local repository, empty when it did not resolve. */
  jar: string;
}

/** One module of the reactor. */
export interface DependencyModule {
  /** `<name>`, else the artifactId. */
  name: string;
  artifact_id: string;
  /** Absolute path of the module's `pom.xml`. */
  pom: string;
  /** `jar` unless the pom says otherwise; `pom` means the module builds nothing. */
  packaging: string;
  dependencies: Dependency[];
}

/** A jar on the classpath that no module declared — something dragged it in. */
export interface TransitiveDependency {
  group_id: string;
  artifact_id: string;
  version: string;
  jar: string;
}

/** Everything the Dependencies panel shows. */
export interface DependencyReport {
  modules: DependencyModule[];
  transitive: TransitiveDependency[];
  /** Whether a resolved classpath was available at all. `false` means the jar column is
   *  **unknown**, not empty — nothing has been resolved yet, and marking every dependency of a
   *  project that builds as missing would be a lie. */
  classpath_known: boolean;
  /** Poms that were found but could not be read. Rare, and worth saying out loud: a missing module
   *  is otherwise indistinguishable from one with no dependencies. */
  unreadable: string[];
}

/** `groupId:artifactId` — the coordinate a person reads. */
export function coordOf(d: { group_id: string; artifact_id: string }): string {
  return d.group_id ? `${d.group_id}:${d.artifact_id}` : d.artifact_id;
}

/** The project's dependencies. Reads poms and the already-resolved classpath; never runs Maven,
 *  so it is safe to call on every panel open. Wire: `bennu_dependencies`. */
export function dependencies(root: string): Promise<DependencyReport> {
  return bennu('bennu_dependencies', { args: { root } });
}
