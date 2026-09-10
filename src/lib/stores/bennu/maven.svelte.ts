/**
 * Bennu's Maven tool window — the reactor as something to look at, and the choices a press
 * carries with it.
 *
 * Two kinds of state, deliberately in one store because they are read together on every press:
 *
 * - **the model** (`bennu_maven_model`): the modules, their plugins, the profiles, the lifecycle.
 *   Read from poms, so re-reading it is cheap and the panel offers a refresh rather than trying
 *   to guess when a pom changed.
 * - **the run options**: which profiles are ticked, and whether tests are skipped. They belong to
 *   the *window*, not to a run — you set them once and then press several goals — which is why
 *   they are here and not in the run spec's defaults.
 *
 * The ticked profiles start as the ones the poms mark `activeByDefault`, because that is what
 * `mvn` would do with no `-P` at all: a panel that started with none ticked would silently run a
 * different build from the terminal.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { mavenModel, type MavenModel, type MavenModule } from '$lib/ipc/bennu/maven-build';

function createMavenStore() {
  let model = $state<MavenModel | null>(null);
  let loading = $state(false);
  let error = $state('');
  let loadedRoot: string | null = null;
  let inFlight: Promise<void> | null = null;

  /** Profile ids the user has ticked. Seeded from the poms' own defaults on each fresh model. */
  let activeProfiles = $state<string[]>([]);
  /** `-DskipTests` on every press from this panel until it is turned back off. */
  let skipTests = $state(false);
  /** `-o` on every press. Off by default — a deliberate goal may need to fetch its own plugin. */
  let offline = $state(false);

  return {
    get model(): MavenModel | null {
      return model;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    /** The reactor, or an empty list before the first read lands. */
    get modules(): MavenModule[] {
      return model?.modules ?? [];
    },
    get activeProfiles(): string[] {
      return activeProfiles;
    },
    get skipTests() {
      return skipTests;
    },
    get offline() {
      return offline;
    },
    /** Whether a Maven launcher was found. `false` is what makes the panel say so once, rather
     *  than letting every press fail to spawn with the reason only in a console tab. */
    get hasLauncher() {
      return !!model && model.launcher.trim() !== '';
    },

    isProfileActive(id: string) {
      return activeProfiles.includes(id);
    },
    toggleProfile(id: string) {
      activeProfiles = activeProfiles.includes(id)
        ? activeProfiles.filter((p) => p !== id)
        : [...activeProfiles, id];
    },
    setSkipTests(v: boolean) {
      skipTests = v;
    },
    setOffline(v: boolean) {
      offline = v;
    },

    /** Read the model for `root`. A repeat call for the same project is a no-op unless `force`;
     *  a call while one is in flight joins it rather than starting a second. */
    async load(root: string, force = false) {
      if (!force && loadedRoot === root && model) return;
      if (inFlight) return inFlight;
      const switching = loadedRoot !== root;
      if (switching) model = null;
      loadedRoot = root;
      loading = true;
      error = '';
      inFlight = (async () => {
        try {
          const next = await mavenModel(root);
          model = next;
          error = '';
          // Seed the ticks on the first model for a project, and never again: re-reading after a
          // pom edit must not undo what the user ticked while looking at it.
          if (switching) {
            activeProfiles = next.profiles.filter((p) => p.active_by_default).map((p) => p.id);
          }
        } catch (e) {
          model = null;
          // The backend's own words — "…has no pom.xml — it is not a Maven project" is the
          // answer, not a failure to be translated into something vaguer.
          error = String(e);
        }
      })();
      try {
        await inFlight;
      } finally {
        inFlight = null;
        loading = false;
      }
    },

    reset() {
      model = null;
      loadedRoot = null;
      error = '';
      activeProfiles = [];
    },
  };
}

export const bennuMavenStore = createMavenStore();
