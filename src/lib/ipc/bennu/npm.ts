/**
 * `package.json` — what it declares, what is behind, and running a script.
 *
 * The npm half of what `cargo.ts` is for a `Cargo.toml`. Three calls, and only the middle one
 * touches the network: it is behind the same *Look packages up online* switch and the same cache
 * TTL as the crates.io lookups, because somebody who turned registry lookups off did not mean
 * "off for Rust".
 */

import { bennu } from '../rpc';
import type { RunHandle } from '$lib/types/bennu';

/** A dependency with a newer release than its range admits. */
export interface NpmVersionHint {
  name: string;
  /** Byte offset of the dependency's name — where the lens is drawn. */
  offset: number;
  /** 1-based line. */
  line: number;
  /** Byte span of the version string's **contents**, quotes EXCLUDED. Unlike the Cargo hint,
   *  whose span includes them — so the replacement here is the bare version. */
  start: number;
  end: number;
  /** The range as written. */
  current: string;
  /** The registry's `latest` dist-tag. */
  latest: string;
}

/** One `scripts` entry. */
export interface NpmScript {
  name: string;
  command: string;
  offset: number;
  line: number;
}

/** What a manifest declares, plus what would run it. */
export interface NpmManifest {
  name: string | null;
  version: string | null;
  scripts: NpmScript[];
  /** `npm` / `yarn` / `pnpm` / `bun`, read off the lockfile beside the manifest. */
  package_manager: string;
}

/** The manifest's scripts and identity, from the BUFFER — so a script added a second ago has its
 *  run control before the file is saved. Wire: `bennu_npm_manifest`. */
export function npmManifest(file: string, source: string): Promise<NpmManifest> {
  return bennu('bennu_npm_manifest', { args: { file, source } });
}

/** Dependencies with a newer release. Empty when registry lookups are off, when offline with a
 *  cold cache, or when nothing is behind. Wire: `bennu_npm_version_hints`. */
export function npmVersionHints(file: string, source: string): Promise<NpmVersionHint[]> {
  return bennu('bennu_npm_version_hints', { args: { file, source } });
}

/** Run one script in a Run console tab, with the project's own package manager.
 *  Wire: `bennu_npm_run_script`. */
export function npmRunScript(root: string, file: string, script: string): Promise<RunHandle> {
  return bennu('bennu_npm_run_script', { args: { root, file, script } });
}
