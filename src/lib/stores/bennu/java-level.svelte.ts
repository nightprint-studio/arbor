/**
 * The Java language level the open project targets.
 *
 * Small, and it earns its place: several editor-side decisions are only correct with it. Postfix
 * templates are the first — `.var` and `.for` want `var` on a modern project and must not emit it on
 * a Java 8 one, where it doesn't compile. Getting that wrong is worse than not offering the template
 * at all, because the code looks right until it is built.
 *
 * The level comes from `bennu_jdk_status`, which reads it out of the build file (`maven.compiler.
 * source`, the Gradle toolchain, …) — the same number the validator gates version-specific features
 * on, so the editor and the diagnostics cannot disagree about what the project is.
 *
 * Defaults to {@link LEGACY_LEVEL} until the answer arrives, and stays there if it never does:
 * Bennu exists for legacy Java, so assuming the old language is the assumption that fails safely.
 */

import { jdkStatus } from '$lib/ipc/bennu/inspect';

/** What the level is taken to be before (or without) an answer — see the module doc. */
export const LEGACY_LEVEL = 8;

function createJavaLevelStore() {
  let level = $state(LEGACY_LEVEL);
  /** The root the current value was loaded for, so switching projects reloads rather than lingers. */
  let loadedRoot = $state<string | null>(null);

  return {
    /** The project's language level — {@link LEGACY_LEVEL} until known. */
    get level() {
      return level;
    },
    /** Whether the project can be given `var` (Java 10) and everything after it. */
    get hasVar() {
      return level >= 10;
    },
    /** Load the level for `root`, unless it is already the one loaded. Never throws. */
    async load(root: string) {
      if (root === loadedRoot) return;
      loadedRoot = root;
      try {
        const status = await jdkStatus(root);
        // `requested_major` is what the project ASKS for, which is what its sources must compile
        // against — not `resolved_major`, which is whichever JDK happened to be installed to run it.
        level = status?.requested_major ?? LEGACY_LEVEL;
      } catch {
        level = LEGACY_LEVEL;
      }
    },
    /** Forget the loaded level — on closing a project, so the next one reloads. */
    reset() {
      level = LEGACY_LEVEL;
      loadedRoot = null;
    },
  };
}

export const javaLevelStore = createJavaLevelStore();
