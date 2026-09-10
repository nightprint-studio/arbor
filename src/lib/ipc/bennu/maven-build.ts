/**
 * The Maven BUILD — what the tool window lists, and the one call that starts Maven.
 *
 * Separate from `maven.ts`, which is about Maven Central: that file answers "is this dependency
 * behind", this one answers "what can I run, and run it". Same product, different question, and
 * one of them opens a socket while the other reads poms.
 *
 * The vocabulary — the lifecycle phases, the goals each plugin binds — comes from the backend
 * rather than being written here, for the reason `cargo.ts` gets its command table from
 * `bennu_cargo_commands`: a panel with its own copy eventually offers something the backend does
 * not run, and the press does nothing with no explanation anywhere.
 */

import { bennu } from '../rpc';

/** One phase of a Maven lifecycle — mirrors the BE `Phase`. */
export interface MavenPhase {
  /** What goes on the command line. */
  id: string;
  /** What it does, in the fewest words that are still true. */
  hint: string;
  /** `clean` · `default` · `site` — which lifecycle it belongs to. `mvn clean install` runs two. */
  lifecycle: string;
}

/** A plugin a pom configures — mirrors the BE `PomPlugin`. */
export interface MavenPlugin {
  group_id: string;
  artifact_id: string;
  /** As written: a `${property}` stays a `${property}`. */
  version: string;
  /** The prefix Maven accepts on a command line (`compiler`), or empty when the artifactId
   *  follows neither conventional shape — then a goal is invoked fully qualified. */
  prefix: string;
  /** The goals its `<execution>`s bind. Empty for a plugin that only carries `<configuration>`,
   *  which is most of them: those run because a lifecycle phase calls them. */
  goals: string[];
  /** Declared under `<pluginManagement>` and nowhere else — configured for the modules rather
   *  than bound in this one, so pressing a goal on it runs nothing here. */
  managed: boolean;
  /** Byte offset of the `<plugin>` tag. */
  offset: number;
  /** 1-based line of it — what the row opens the pom at. */
  line: number;
}

/** One module of the reactor — mirrors the BE `MavenModule`. */
export interface MavenModule {
  /** Directory relative to the project root, forward-slashed. Empty for the root module. */
  dir: string;
  artifact_id: string;
  /** `<name>` when the pom gives one; empty otherwise, and then the artifactId is the name. */
  name: string;
  /** `jar` / `war` / `pom` / …; empty means `jar`. */
  packaging: string;
  /** Absolute path of its pom. */
  pom: string;
  plugins: MavenPlugin[];
}

/** A profile declared somewhere in the reactor — mirrors the BE `PomProfile`. */
export interface MavenProfile {
  id: string;
  /** The pom marks it `activeByDefault`. NOT "active now": every other activation Maven supports
   *  is a fact about the machine or the command line. */
  active_by_default: boolean;
  /** The module that declares it, relative to the root. */
  module: string;
}

/** Everything the Maven tool window draws — mirrors the BE `MavenModel`. */
export interface MavenModel {
  /** The launcher that would be used, or empty when none was found — the panel's cue to say so
   *  once, instead of every press failing to spawn with nothing on screen. */
  launcher: string;
  /** The reactor, root first. */
  modules: MavenModule[];
  /** Every profile in the reactor, deduplicated by id. */
  profiles: MavenProfile[];
  /** The lifecycle phases, in run order. */
  lifecycle: MavenPhase[];
}

/** The reactor, its plugins and its profiles. Reads poms — no Maven, no network, so it is cheap
 *  enough to call every time the panel opens. Rejects when the root has no `pom.xml`.
 *  Wire: `bennu_maven_model`. */
export function mavenModel(root: string): Promise<MavenModel> {
  return bennu('bennu_maven_model', { args: { root } });
}

/** What a press asks for beyond the goals themselves. */
export interface MavenGoalOptions {
  /** The module to run in, relative to the root. Empty = the root, i.e. the whole reactor. */
  module?: string;
  /** Profile ids to activate (`-P a,b`). */
  profiles?: string[];
  /** `-DskipTests`. */
  skipTests?: boolean;
  /** `-o`. Off by default: a deliberate press may legitimately need to fetch the plugin that
   *  performs the goal. */
  offline?: boolean;
}

/** Run Maven goals in a module, streaming into the Run console. Returns immediately with the
 *  handle the console correlates by; Stop and stdin work on it exactly as for a JVM run.
 *
 *  `goals` are phases and plugin goals as they go on the command line — options are the fields
 *  above, and the backend refuses a goal that starts with `-`.
 *  Wire: `bennu_maven_goal`. */
export function mavenGoal(
  root: string,
  goals: string[],
  opts: MavenGoalOptions = {},
): Promise<{ run_id: string; main_class: string; command: string; working_dir: string }> {
  return bennu('bennu_maven_goal', {
    args: {
      root,
      goals,
      module: opts.module ?? '',
      profiles: opts.profiles ?? [],
      skip_tests: opts.skipTests ?? false,
      offline: opts.offline ?? false,
    },
  });
}
