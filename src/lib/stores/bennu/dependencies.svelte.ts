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

import { projectAdvisories } from '$lib/ipc/bennu/maven';
import {
  dependencies as fetchDependencies,
  dependencyTree as fetchTree,
  type DependencyReport,
  type DependencyTree,
} from '$lib/ipc/bennu/deps';

/** `group:artifact` → the versions of it that have advisories, with how many and how bad. */
export type VulnIndex = Map<string, { version: string; count: number; severity: string }[]>;

const SEVERITY_RANK: Record<string, number> = { critical: 5, high: 4, moderate: 3, medium: 3, unknown: 2, low: 1 };

function worstSeverity(severities: string[]): string {
  return severities.reduce(
    (worst, s) => ((SEVERITY_RANK[s.toLowerCase()] ?? 2) > (SEVERITY_RANK[worst.toLowerCase()] ?? 0) ? s : worst),
    'low',
  );
}

/** The badge for a dependency: that exact version when it is known, else the worst of its versions
 *  (a BOM-managed row has no version of its own to compare). */
export function vulnFor(
  index: VulnIndex, coord: string, version: string,
): { count: number; severity: string } | undefined {
  const versions = index.get(coord);
  if (!versions?.length) return undefined;
  const hits = version ? versions.filter((v) => v.version === version) : versions;
  if (hits.length === 0) return undefined;
  return {
    count: hits.reduce((n, h) => n + h.count, 0),
    severity: worstSeverity(hits.map((h) => h.severity)),
  };
}

function createDependenciesStore() {
  let report = $state<DependencyReport | null>(null);
  let loading = $state(false);
  let error = $state('');
  let loadedRoot: string | null = null;
  /** Last report per project, so swapping back to an open project is instant. */
  const byRoot = new Map<string, DependencyReport>();

  // The resolved tree runs Maven, so it is kept apart from the report (which never does) and only
  // read when asked for. One per project; a run survives a project switch and lands in its cache.
  let tree = $state<DependencyTree | null>(null);
  let treeLoading = $state(false);
  let treeError = $state('');
  let treeRoot: string | null = null;
  const treeByRoot = new Map<string, DependencyTree>();
  const treeInFlight = new Map<string, Promise<void>>();

  let vulns = $state<VulnIndex>(new Map());
  const vulnInFlight = new Map<string, Promise<void>>();
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
      // Switching projects shows the last report we had for the target at once and refreshes
      // behind it (stale-while-revalidate), so a project swap never blanks the panel.
      if (loadedRoot !== root) {
        report = byRoot.get(root) ?? null;
        vulns = new Map(); // another project's badges would be wrong here
      }
      loadedRoot = root;
      loading = true;
      error = '';
      inFlight = (async () => {
        try {
          const fresh = await fetchDependencies(root);
          byRoot.set(root, fresh);
          // The user may have switched away while this was in the air.
          if (loadedRoot === root) report = fresh;
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

    /** The resolved tree of the current project, once asked for. Null until then: it runs Maven. */
    get tree(): DependencyTree | null {
      return tree;
    },
    get treeLoading() {
      return treeLoading;
    },
    get treeError() {
      return treeError;
    },

    /** Ask Maven for the resolved tree of `root` in the background. A repeat call while one is
     *  running joins it; a finished answer is kept per project, and `force` replaces it. */
    async loadTree(root: string, force = false) {
      treeRoot = root;
      const cached = treeByRoot.get(root);
      if (cached && !force) {
        tree = cached;
        treeError = '';
        return;
      }
      if (treeInFlight.has(root)) return treeInFlight.get(root);
      tree = cached ?? null;
      treeLoading = true;
      treeError = '';
      const run = (async () => {
        try {
          const fresh = await fetchTree(root);
          treeByRoot.set(root, fresh);
          if (treeRoot === root) tree = fresh;
        } catch (e) {
          if (treeRoot === root) treeError = String(e);
        } finally {
          treeInFlight.delete(root);
          if (treeRoot === root) treeLoading = false;
        }
      })();
      treeInFlight.set(root, run);
      return run;
    },

    /** Known vulnerabilities of `root`'s classpath, by artifact — what the rows badge. Empty until
     *  loaded, and when the check is off. */
    get advisories(): VulnIndex {
      return vulns;
    },

    /** Ask OSV (through the backend's cache) which classpath artifacts are vulnerable. Cheap when
     *  the check is off or the answer is cached; never throws — a missing badge is not an error. */
    async loadAdvisories(root: string) {
      if (vulnInFlight.has(root)) return vulnInFlight.get(root);
      const run = (async () => {
        try {
          const res = await projectAdvisories(root);
          const next: VulnIndex = new Map();
          for (const item of res.items) {
            const list = next.get(item.coord) ?? [];
            list.push({ version: item.version, count: item.advisories.length, severity: worstSeverity(item.advisories.map((a) => a.severity)) });
            next.set(item.coord, list);
          }
          if (treeRoot === root || loadedRoot === root) vulns = next;
        } catch {
          /* no badge */
        } finally {
          vulnInFlight.delete(root);
        }
      })();
      vulnInFlight.set(root, run);
      return run;
    },

    /** Show whatever is already known for `root` without starting Maven (project switch). */
    adoptTree(root: string) {
      treeRoot = root;
      tree = treeByRoot.get(root) ?? null;
      treeError = '';
      treeLoading = treeInFlight.has(root);
    },

    reset() {
      vulns = new Map();
      tree = null;
      treeError = '';
      treeLoading = false;
      report = null;
      loadedRoot = null;
      error = '';
      again = false;
    },
  };
}

export const dependenciesStore = createDependenciesStore();
