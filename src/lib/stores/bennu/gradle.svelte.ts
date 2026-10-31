/**
 * Bennu's Gradle tool window — the build as something to look at, and the choices a press carries.
 *
 * The counterpart of `maven.svelte.ts`, minus profiles (Gradle has no such thing) and with the one
 * toggle that matters on every press — `-x test`. Two kinds of state in one store because they are
 * read together:
 *
 * - **the model** (`bennu_gradle_model`): projects, plugins, tasks. Read from build scripts, so
 *   re-reading is cheap and the panel offers a refresh instead of guessing when a script changed.
 * - **the run options**: whether tests are skipped. They belong to the *window*, not to a run — you
 *   set them once and then press several tasks.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { gradleModel, type GradleModel, type GradleModule } from '$lib/ipc/bennu/gradle';
import { gradleLiveKeys } from '$lib/components/bennu/tree-expansion';
import { treeExpansionStore } from './tree-expansion.svelte';
import { bennuSettingsStore } from './settings.svelte';

function createGradleStore() {
  let model = $state<GradleModel | null>(null);
  let loading = $state(false);
  let error = $state('');
  let loadedRoot: string | null = null;
  let inFlight: Promise<void> | null = null;

  /** `-x test` on every press from this panel until it is turned back off. */
  let skipTests = $state(false);

  return {
    get model(): GradleModel | null {
      return model;
    },
    get loading() {
      return loading;
    },
    get error() {
      return error;
    },
    /** The build's projects, or an empty list before the first read lands. */
    get modules(): GradleModule[] {
      return model?.modules ?? [];
    },
    get skipTests() {
      return skipTests;
    },
    /**
     * `--offline` on every press. There is ONE setting for "may the build tool use the network"
     * (`maven_auto_download`, in the Java settings) and it covers Gradle too: this panel's toggle is
     * a second way to flip it, not a second setting — two switches for one question had a task here
     * going online while the classpath beside it stayed offline.
     */
    get offline() {
      return !bennuSettingsStore.mavenAutoDownload;
    },
    /** Whether a launcher (a `gradlew` or a `gradle` on the PATH) was found. */
    get hasLauncher() {
      return !!model && model.launcher.trim() !== '';
    },

    setSkipTests(v: boolean) {
      skipTests = v;
    },
    setOffline(v: boolean) {
      bennuSettingsStore.setMavenAutoDownload(!v);
    },

    /** Read the model for `root`. A repeat call for the same project is a no-op unless `force`; a
     *  call while one is in flight joins it rather than starting a second. */
    async load(root: string, force = false) {
      if (!force && loadedRoot === root && model) return;
      if (inFlight) return inFlight;
      if (loadedRoot !== root) model = null;
      loadedRoot = root;
      loading = true;
      error = '';
      inFlight = (async () => {
        try {
          const next = await gradleModel(root);
          model = next;
          error = '';
          // A complete model is the one moment the remembered rows can be checked against what
          // exists — a project removed from settings.gradle stops being remembered.
          treeExpansionStore.prune(root, 'gradle', gradleLiveKeys(next));
        } catch (e) {
          model = null;
          // The backend's own words — "…has no settings.gradle or build.gradle" is the answer.
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
    },
  };
}

export const bennuGradleStore = createGradleStore();
