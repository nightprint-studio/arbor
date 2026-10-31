/**
 * The Gradle BUILD — what the tool window lists, and the calls that start Gradle.
 *
 * The Gradle counterpart of `maven-build.ts`, and shaped like it: the model is read from the build
 * files (`settings.gradle`, every `build.gradle[.kts]`), so it is cheap enough to ask every time
 * the panel opens; Gradle itself is started only by the verbs below.
 *
 * The task table comes from the backend rather than being written here, for the reason the Maven
 * lifecycle and `cargo.ts`'s command table do: a panel with its own copy eventually offers
 * something the backend does not run.
 */

import { bennu } from '../rpc';

/** One runnable task — mirrors the BE `bennu_gradle::Task`. */
export interface GradleTask {
  /** What goes on the command line. */
  id: string;
  /** What it does, in the fewest words that are still true. */
  hint: string;
  /** `build` · `verification` · `application` · `documentation` · `publishing` · `help` · `other`. */
  group: string;
}

/** A plugin a build script applies or declares — mirrors the BE `GradlePluginRow`. */
export interface GradlePlugin {
  id: string;
  /** Empty when the script names none. */
  version: string;
  /** `false` for `apply false`: declared so subprojects can use it, not applied here. */
  apply: boolean;
  /** 1-based line in the build script — what the row opens it at. */
  line: number;
}

/** One project of the build — mirrors the BE `GradleModuleRow`. */
export interface GradleModule {
  /** The Gradle path: `:` for the root, `:api`, `:web:client`. */
  path: string;
  name: string;
  /** Directory relative to the build root, forward-slashed. Empty for the root. */
  dir: string;
  /** Absolute path of its build script; empty for a project that has none (a container). */
  build_file: string;
  /** `jar` · `war` · `platform`, or empty for a project that applies no JVM plugin. */
  packaging: string;
  plugins: GradlePlugin[];
  tasks: GradleTask[];
}

/** A version catalog the build reads — mirrors the BE `GradleCatalogRow`. */
export interface GradleCatalog {
  name: string;
  file: string;
  libraries: number;
}

/** Everything the Gradle tool window draws — mirrors the BE `GradleModel`. */
export interface GradleModel {
  /** The launcher that would be used, or empty when none was found — the panel's cue to say so once. */
  launcher: string;
  /** The build has its own `gradlew`. */
  wrapper: boolean;
  build_name: string;
  /** Root first, then the projects `settings.gradle` includes. */
  modules: GradleModule[];
  catalogs: GradleCatalog[];
  /** Absolute path of the settings script; empty when the build has none. */
  settings_file: string;
}

/** The build's projects, plugins and tasks. Reads build files — no Gradle, no network. Rejects when
 *  the root has no Gradle script. Wire: `bennu_gradle_model`. */
export function gradleModel(root: string): Promise<GradleModel> {
  return bennu('bennu_gradle_model', { args: { root } });
}

/** What a press asks for beyond the tasks themselves. */
export interface GradleTaskOptions {
  /** The project to run in, as a Gradle path (`:api`). Empty = the root, i.e. the whole build. */
  module?: string;
  /** `-x test`. */
  skipTests?: boolean;
  /** `--offline`. */
  offline?: boolean;
}

/** Run Gradle tasks, streaming into the Run console. Returns immediately with the handle the
 *  console correlates by; Stop and stdin work on it exactly as for a JVM run.
 *
 *  `tasks` are task names as they go on the command line — options are the fields above, and the
 *  backend refuses a task that starts with `-`. Wire: `bennu_gradle_task`. */
export function gradleTask(
  root: string,
  tasks: string[],
  opts: GradleTaskOptions = {},
): Promise<{ run_id: string; main_class: string; command: string; working_dir: string }> {
  return bennu('bennu_gradle_task', {
    args: {
      root,
      tasks,
      module: opts.module ?? '',
      skip_tests: opts.skipTests ?? false,
      offline: opts.offline ?? false,
    },
  });
}

/** Resolve every classpath of the build with the network allowed, then rebuild the index — the
 *  Gradle `go-offline`. Resolves with `started`; the work is a job. Wire: `bennu_gradle_download`. */
export function gradleDownload(root: string): Promise<string> {
  return bennu('bennu_gradle_download', { args: { root } });
}

/** Drop the cached classpath and rebuild the index behind it. Wire: `bennu_gradle_reload`. */
export function gradleReload(root: string): Promise<string> {
  return bennu('bennu_gradle_reload', { args: { root } });
}
