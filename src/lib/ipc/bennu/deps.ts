/**
 * Bennu dependencies IPC — what the project depends on, and who decided each answer.
 *
 * Mirrors `bennu-deps`'s model field-for-field in **snake_case**; the Rust side
 * (`crates/products/bennu/deps/src/model.rs`) is authoritative, and its module doc has the table
 * mapping each field onto Maven's vocabulary and Cargo's.
 *
 * **One shape, two ecosystems.** The same report describes a Maven reactor and a Cargo workspace,
 * because the questions a dependency list answers are the same ones; {@link DependencyReport.ecosystem}
 * says which it is, so the panel can label a column `scope` or `kind` instead of pretending the two
 * words mean the same thing.
 *
 * Kept in its own file (not `index.ts`) so concurrent edits to the main bennu IPC surface don't
 * race — same reasoning as `ext.ts`.
 */

import { bennu } from '../rpc';

/** Where a dependency's presence — or its version — was decided.
 *
 *  Three genuinely different facts, which is why this is a union and not a boolean:
 *  `declared` needs no explanation, `managed` means the module asked for the artifact and something
 *  further up chose the version (a `<dependencyManagement>` entry, or Cargo's `workspace = true`),
 *  and `inherited` means the module never mentioned it at all — a parent pom's own `<dependencies>`
 *  are every child's. */
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
  /** Maven's `groupId` — half the coordinate. Empty for Cargo, whose crate names are flat. */
  group: string;
  /** Cargo: where the crate comes from (`crates.io`, `path`, `git`, `workspace`, a registry).
   *  Empty for Maven. Provenance, not identity — which is why it is not folded into `group`. */
  source: string;
  /** Maven's `artifactId`, or the crate name as this manifest refers to it (the local name for a
   *  renamed dependency). */
  name: string;
  /** The version actually in effect: `${…}` expanded and `<dependencyManagement>` applied (Maven),
   *  or the requirement replaced by what `Cargo.lock` chose (Cargo). **Empty when nothing on disk
   *  answers it** — an imported BOM, a parent that only exists in the repository, a
   *  `[workspace.dependencies]` entry that is not there. Never guessed. */
  version: string;
  /** When it is needed. Maven: `compile` when the pom doesn't say. Cargo: `normal` · `dev` ·
   *  `build`. */
  scope: string;
  /** Maven's `<type>` when it isn't the default `jar`. Empty for Cargo. */
  kind: string;
  /** Maven's `<classifier>`, or — for Cargo — the real crate name when this entry renames it.
   *  Both are "the same coordinate, a different artifact behind it". */
  variant: string;
  optional: boolean;
  origin: DependencyOrigin;
  /** What must be true for it to be on the graph at all, empty for the ordinary case. Maven: the
   *  profile it came from. Cargo: the `cfg(…)` of a target table. Neither can be evaluated here. */
  condition: string;
  /** Cargo only: the features this manifest turns on for the dependency. */
  features: string[];
  /** The manifest that declares it, and where — so the row is somewhere to go. */
  declared_in: DependencySite;
  /** Where the artifact actually is: the jar in `~/.m2`, or the crate's unpacked source in the
   *  local Cargo registry. Empty when it did not resolve. */
  resolved: string;
}

/** One module (Maven) or crate (Cargo) of the project. */
export interface DependencyModule {
  /** `<name>`, else the artifactId; the crate's name for Cargo. */
  name: string;
  /** What the build tool knows it by — the artifactId, or the crate name. */
  id: string;
  /** Absolute path of the module's manifest (`pom.xml` / `Cargo.toml`). */
  manifest: string;
  /** What it builds. Maven: `<packaging>` — `pom` means the module builds nothing. Cargo: the
   *  target kinds it has (`lib`, `bin`, `lib+bin`, `proc-macro`). */
  kind: string;
  dependencies: Dependency[];
}

/** An artifact on the resolved graph that no module declared — something dragged it in. */
export interface TransitiveDependency {
  /** Maven's `groupId`. Empty for Cargo. */
  group: string;
  name: string;
  version: string;
  /** The jar, or the crate's unpacked source. */
  resolved: string;
}

/** Everything the Dependencies panel shows. */
export interface DependencyReport {
  /** `maven` or `cargo` — which build tool this describes. Empty for a project that is neither. */
  ecosystem: string;
  modules: DependencyModule[];
  transitive: TransitiveDependency[];
  /** Whether resolved artifacts were available at all. `false` means the `resolved` column is
   *  **unknown**, not empty — nothing has been resolved yet (Maven resolves in the background as
   *  the project indexes; Cargo has no `Cargo.lock`) — and marking every dependency of a project
   *  that builds as missing would be a lie. */
  resolved_known: boolean;
  /** Manifests that were found but could not be read. Rare, and worth saying out loud: a missing
   *  module is otherwise indistinguishable from one with no dependencies. */
  unreadable: string[];
}

/** The coordinate a person reads: `group:name` where there is a group, else just the name.
 *
 *  One function for both ecosystems, because `group` is only ever a namespace — a Maven dependency
 *  reads `org.springframework:spring-web` and a Cargo one reads `serde`. */
export function coordOf(d: { group: string; name: string }): string {
  return d.group ? `${d.group}:${d.name}` : d.name;
}

/** What the ecosystem calls a dependency's scope column. Cargo's three values are *kinds*, not
 *  scopes, and calling them the same thing would be a small lie in the one place the panel is
 *  supposed to be precise. */
export function scopeLabel(ecosystem: string): string {
  return ecosystem === 'cargo' ? 'Kind' : 'Scope';
}

/** The project's dependencies. Reads poms and the already-resolved classpath; never runs Maven,
 *  so it is safe to call on every panel open. On a Cargo root it reads the workspace manifests and
 *  `Cargo.lock` instead, and never runs cargo. Wire: `bennu_dependencies`. */
export function dependencies(root: string): Promise<DependencyReport> {
  return bennu('bennu_dependencies', { args: { root } });
}
