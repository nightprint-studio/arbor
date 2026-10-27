/**
 * Maven Central — the one question about a pom that this machine cannot answer.
 *
 * Everything else Bennu knows about a Maven project is read off the disk: the poms, `~/.m2`, the
 * classpath resolved from them. Not this one. A local repository holds what somebody here has
 * already asked for, so by its measure a dependency nobody has ever updated is permanently current
 * — which is exactly the dependency worth saying something about.
 *
 * The mirror of `$lib/ipc/bennu/cargo`'s version hints, and shaped like it on purpose: same lens,
 * same arrow, same debounce, so a reader who has learnt what `↑ 6.1.14 available` means above a
 * `Cargo.toml` does not have to learn it again above a `pom.xml`.
 *
 * Behind a switch (`[maven] central` in the config) and a day-long cache — see the BE's
 * `maven_central.rs`.
 */

import { bennu } from '../rpc';

/** Whether a path is a Maven pom — the gate for asking about it at all.
 *
 *  The **name**, not the extension: a Java project is full of XML that is not a manifest, and
 *  offering to bump a version inside a Spring context would be offering to break it. */
export function isPomFile(path: string | null | undefined): boolean {
  if (!path) return false;
  return (path.split(/[\\/]/).pop() ?? '').toLowerCase() === 'pom.xml';
}

/** One dependency that has a newer release on Central — mirrors the BE `PomVersionHint`. */
export interface PomVersionHint {
  /** `groupId:artifactId`, for a message that names what moved. */
  coord: string;
  /** Byte offset of the `<dependency>` tag — where the lens row is drawn. */
  offset: number;
  /** 1-based line of that tag. */
  line: number;
  /** Byte span of the version **value**, tags excluded — what accepting the hint replaces. */
  start: number;
  end: number;
  /** The version the pom writes. */
  current: string;
  /** The newest release on Central. */
  latest: string;
}

/** Which of a pom's pinned dependencies are behind. Resolves to `[]` — never rejects — when the
 *  switch is off, when nothing is behind, or when Central cannot be reached and nothing is cached.
 *
 *  Only what the pom itself pins: a version inherited from a parent or a BOM, or written as a
 *  `${property}`, is left alone, because the line this would draw above is not the line that would
 *  have to change.
 *  Wire: `bennu_maven_version_hints` — `{ file, source }`. */
export function pomVersionHints(file: string, source: string): Promise<PomVersionHint[]> {
  return bennu('bennu_maven_version_hints', { args: { file, source } });
}

/** One known vulnerability — mirrors the BE `Advisory`. */
export interface Advisory {
  /** OSV's id (`GHSA-…`). */
  id: string;
  /** The CVE alias, when there is one. */
  cve: string | null;
  summary: string;
  /** `critical` | `high` | `moderate` | `low` | `unknown`. */
  severity: string;
  /** The nearest version above the current one that fixes it, when the record says. */
  fixed: string | null;
}

/** A dependency of the pom with at least one known vulnerability — mirrors the BE `PomAdvisory`. */
export interface PomAdvisory {
  coord: string;
  line: number;
  /** Byte span of the version **value** — where the warning is drawn. */
  start: number;
  end: number;
  current: string;
  advisories: Advisory[];
}

/** Which of a pom's pinned dependencies have known vulnerabilities (OSV.dev). Resolves to `[]` —
 *  never rejects — when the switch is off, nothing is affected, or OSV cannot be reached.
 *
 *  Opt-in (`[maven] advisories`), cached for a day, bounded per request. Wire:
 *  `bennu_maven_advisories` — `{ file, source }`. */
export function pomAdvisories(file: string, source: string): Promise<PomAdvisory[]> {
  return bennu('bennu_maven_advisories', { args: { file, source } });
}

/** One classpath artifact with known vulnerabilities — mirrors the BE `ProjectAdvisory`. */
export interface ProjectAdvisory {
  coord: string;
  /** The version the build resolved, not necessarily the one a pom writes. */
  version: string;
  /** Declared by a module, as opposed to dragged in transitively. */
  direct: boolean;
  modules: string[];
  /** Declaration site of a direct dependency; empty / 0 otherwise. */
  file: string;
  line: number;
  advisories: Advisory[];
}

/** The whole project's vulnerabilities — mirrors the BE `ProjectAdvisories`. */
export interface ProjectAdvisories {
  /** `[maven] advisories` is on. When false, nothing was sent and `items` is empty. */
  enabled: boolean;
  checked: number;
  /** More artifacts than one pass covers. */
  truncated: boolean;
  items: ProjectAdvisory[];
}

/** Known vulnerabilities across everything on the project's classpath (OSV.dev).
 *  Wire: `bennu_project_advisories` — `{ root }`. */
export function projectAdvisories(root: string): Promise<ProjectAdvisories> {
  return bennu('bennu_project_advisories', { args: { root } });
}
