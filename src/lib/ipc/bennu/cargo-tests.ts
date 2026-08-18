/**
 * Cargo test IPC — discovery + running, over the generic `bennu(...)` bridge.
 *
 * Two calls, because almost everything a run produces arrives as **events**:
 * `bennu_run_cargo_tests` resolves as soon as cargo is up, and the targets, the cases, the output
 * and the exit all stream on `arbor://bennu/cargo-test-{target,case,target-done,compiling}` plus
 * the shared `arbor://bennu/test-{output,exit}`. See `bennu-be/src/cargo_tests.rs` for the
 * contract and `$lib/stores/bennu/cargo-tests.svelte.ts` for the subscriber.
 *
 * Stopping a run is **`cancelTests`** from the Maven module — one registry in the backend, one
 * verb here, so the Stop button never has to pick.
 */

import { bennu } from '../rpc';
import type { CargoRunHandle, CargoTestScope, DiscoveredRustTest } from '$lib/types/bennu';

/**
 * Every `#[test]` in the workspace — or, with `file`, just that one file's.
 *
 * Answers from the file **on disk**: cargo compiles from disk, so a test discovered in unsaved
 * text is one the runner cannot run. Whole-workspace results are cached on the backend (the walk
 * reads every `.rs` in the tree); `force` re-scans.
 *
 * Wire: `bennu_discover_cargo_tests` — `DiscoverArgs { root, file?, force? }`.
 */
export function discoverCargoTests(
  root: string,
  opts: { file?: string; force?: boolean } = {},
): Promise<DiscoveredRustTest[]> {
  return bennu('bennu_discover_cargo_tests', {
    args: { root, file: opts.file ?? null, force: opts.force ?? false },
  });
}

/** Launch `cargo test` for `scope`. Resolves once cargo is up — everything after that arrives as
 *  events. Wire: `bennu_run_cargo_tests` — `RunArgs { root, scope, include_ignored }`. */
export function runCargoTests(
  root: string,
  scope: CargoTestScope,
  includeIgnored = false,
): Promise<CargoRunHandle> {
  return bennu('bennu_run_cargo_tests', {
    args: { root, scope, include_ignored: includeIgnored },
  });
}
