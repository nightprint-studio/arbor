/**
 * Bennu dependencies — the project's real dependency set, per module, from `bennu_dependencies`.
 *
 * One report per project, fetched on demand and cached until the project changes or something
 * asks for it again. Cheap enough to re-read on request (the backend reads poms and a cached
 * classpath list — it never runs Maven), which is why the panel offers a refresh instead of
 * guessing when to invalidate: a pom edit and a background classpath resolve both change the
 * answer, and neither is worth a watcher.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { dependencies as fetchDependencies, type DependencyReport } from '$lib/ipc/bennu/deps';

function createDependenciesStore() {
  let report = $state<DependencyReport | null>(null);
  let loading = $state(false);
  let error = $state('');
  let loadedRoot: string | null = null;
  /** The read in flight, so a burst of `load` calls costs one backend round-trip and not one
   *  each. A store that answers a stampede by starting a request per caller turns a noisy
   *  dependency into a backend that stops answering — every request gets its own thread over the
   *  wire, and a whole-reactor pom walk is not free. */
  let inFlight: Promise<void> | null = null;
  /** Something asked for a refresh while one was running: do exactly one more when it lands,
   *  rather than dropping the request (the answer may have changed) or running N. */
  let again = false;

  return {
    get report(): DependencyReport | null {
      return report;
    },
    get loading() {
      return loading;
    },
    /** A user-facing reason the report could not be read, empty when there is none. The backend
     *  answers "no poms" with an empty report rather than an error, so this really is exceptional
     *  — a dead backend, a project that vanished. */
    get error() {
      return error;
    },

    /** Fetch the report for `root`. A repeat call for the same project is a no-op unless `force`,
     *  and a call while one is already running joins it instead of starting a second. */
    async load(root: string, force = false) {
      if (!force && loadedRoot === root && report) return;
      if (inFlight) {
        again = true;
        return inFlight;
      }
      if (loadedRoot !== root) report = null;
      loadedRoot = root;
      loading = true;
      error = '';
      inFlight = (async () => {
        try {
          report = await fetchDependencies(root);
          error = '';
        } catch (e) {
          report = null;
          error = String(e);
        }
      })();
      try {
        await inFlight;
      } finally {
        inFlight = null;
        loading = false;
      }
      // One catch-up read for everything that asked while this was in the air.
      if (again && loadedRoot === root) {
        again = false;
        await this.load(root, true);
      }
      again = false;
    },

    reset() {
      report = null;
      loadedRoot = null;
      error = '';
      again = false;
    },
  };
}

export const dependenciesStore = createDependenciesStore();
