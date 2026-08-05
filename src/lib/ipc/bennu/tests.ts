/**
 * Bennu unit-test IPC — discovery + running, over the generic `bennu(...)` bridge.
 *
 * Only three calls, because almost everything a test run produces arrives as **events**
 * rather than as a return value: `bennu_run_tests` resolves as soon as Maven is up, and the
 * output, the per-class results and the exit all stream on
 * `arbor://bennu/test-{output,running,class,exit}`. See `bennu-be/src/tests.rs` for the
 * contract and `$lib/stores/bennu/tests.svelte.ts` for the subscriber.
 */

import { bennu } from '../rpc';
import type { DiscoveredTest, TestRunHandle, TestScope } from '$lib/types/bennu';

/**
 * Every test class in the project — or, with `file`, just that one file's.
 *
 * Answers from the file **on disk**: Maven compiles from disk, so a test discovered in
 * unsaved text is one the runner cannot run. Whole-project results are cached on the
 * backend (the walk parses every `.java` in the tree); `force` re-scans.
 *
 * Wire: `bennu_discover_tests` — `DiscoverTestsArgs { root, file?, force? }`.
 */
export function discoverTests(
  root: string,
  opts: { file?: string; force?: boolean } = {},
): Promise<DiscoveredTest[]> {
  return bennu('bennu_discover_tests', {
    args: { root, file: opts.file ?? null, force: opts.force ?? false },
  });
}

/** Launch `mvn test` for `scope`. Resolves once the child is up — everything after that
 *  arrives as events. Wire: `bennu_run_tests` — `RunTestsArgs { root, scope }`. */
export function runTests(root: string, scope: TestScope): Promise<TestRunHandle> {
  return bennu('bennu_run_tests', { args: { root, scope } });
}

/** Kill a live test run and everything it started. `false` when the id is unknown or the
 *  run had already finished. Wire: `bennu_cancel_tests` — `CancelTestsArgs { run_id }`. */
export function cancelTests(runId: string): Promise<boolean> {
  return bennu('bennu_cancel_tests', { args: { run_id: runId } });
}
