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
 * ## The module wins over the project
 *
 * A reactor part-way through a migration has one module on 21 and another still on 8, and that is
 * an ordinary state. The index and the classpath are one JDK by construction — there is one
 * `rt.jar` in an index — but whether a syntax exists yet is a question about the file in front of
 * you, and both the validator and the postfix templates ask it per file. So the open file's own
 * module answers when its pom declares a level, and the project answers when it does not.
 *
 * Defaults to {@link LEGACY_LEVEL} until the answer arrives, and stays there if it never does:
 * Bennu exists for legacy Java, so assuming the old language is the assumption that fails safely.
 */

import { jdkStatus, moduleJdk, type ModuleJdk } from '$lib/ipc/bennu/inspect';

/** What the level is taken to be before (or without) an answer — see the module doc. */
export const LEGACY_LEVEL = 8;

function createJavaLevelStore() {
  let level = $state(LEGACY_LEVEL);
  /** The root the current value was loaded for, so switching projects reloads rather than lingers. */
  let loadedRoot = $state<string | null>(null);
  /** What the PROJECT answered — what the level falls back to when the open file's module says
   *  nothing, and what the level returns to when no file is open. */
  let projectLevel = $state(LEGACY_LEVEL);
  /** The open file's module, when its own pom declares a level. `null` means the project's answer
   *  stands, which is every single-module project. */
  let module = $state<ModuleJdk | null>(null);
  /** The file `module` was loaded for, so a tab switch reloads rather than lingers. */
  let loadedFile = $state<string | null>(null);

  return {
    /** The language level in force for the open file — its module's when that declares one, else
     *  the project's; {@link LEGACY_LEVEL} until known. */
    get level() {
      return level;
    },
    /** The open file's module when it declares a level of its own, else `null`. What the status bar
     *  names beside the level, so a number that differs from the project's says why. */
    get module() {
      return module;
    },
    /** The level the PROJECT declares, whatever the open file's module says. */
    get projectLevel() {
      return projectLevel;
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
        projectLevel = status?.requested_major ?? LEGACY_LEVEL;
      } catch {
        projectLevel = LEGACY_LEVEL;
      }
      level = module?.major ?? projectLevel;
    },
    /** Load the level of `file`'s own module. Never throws; `null` clears back to the project's. */
    async loadFile(file: string | null) {
      if (file === loadedFile) return;
      loadedFile = file;
      if (!file) {
        module = null;
        level = projectLevel;
        return;
      }
      try {
        const found = await moduleJdk(file);
        // A late answer for a tab the user has already left must not overwrite the current one.
        if (loadedFile !== file) return;
        module = found?.major != null ? found : null;
      } catch {
        if (loadedFile !== file) return;
        module = null;
      }
      level = module?.major ?? projectLevel;
    },
    /** Forget the loaded level — on closing a project, so the next one reloads. */
    reset() {
      level = LEGACY_LEVEL;
      projectLevel = LEGACY_LEVEL;
      module = null;
      loadedRoot = null;
      loadedFile = null;
    },
  };
}

export const javaLevelStore = createJavaLevelStore();
