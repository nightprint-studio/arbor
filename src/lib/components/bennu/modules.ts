/**
 * Which module a file belongs to — the one question a reactor makes you ask constantly.
 *
 * On a legacy multi-module build "the `OrderDao`" is four classes and "the `web.xml`" is
 * eleven files; the module is what tells them apart, so both the go-to overlay and Find in
 * project narrow by it. This lives here rather than in either of them because two copies of a
 * path-matching rule drift silently: one normalises `./`, the other does not, and the same file
 * ends up filed under two different modules depending on which box you opened.
 */
import type { ProjectInfo } from '$lib/types/bennu';

/** A project's modules, normalised and **longest path first**. */
export function moduleList(project: ProjectInfo | null | undefined): string[] {
  return [...(project?.modules ?? [])]
    .map((m) => m.replace(/\\/g, '/').replace(/^\.\//, '').replace(/\/+$/, ''))
    // Longest-first is what makes a nested module win over the parent that lists it: on a tree
    // with `modules/core` and `modules`, a class in the first must not be filed under the second.
    .sort((a, b) => b.length - a.length);
}

/** A project-relative, forward-slashed path — the part that tells two files apart. */
export function relativeTo(root: string | null | undefined, file: string): string {
  const base = (root ?? '').replace(/\\/g, '/').replace(/\/+$/, '');
  const norm = file.replace(/\\/g, '/');
  return base && norm.startsWith(`${base}/`) ? norm.slice(base.length + 1) : norm;
}

/**
 * A lookup over one project's modules.
 *
 * Built once from the project rather than resolved per call: the caller runs it over every file
 * in a tree (or every hit in a scan), and re-deriving the sorted list inside the loop is the
 * cheapest possible way to make a filter feel slow.
 */
export interface ModuleIndex {
  /** The modules, longest first. Empty on a single-module project. */
  readonly modules: string[];
  /** In alphabetical order — what a picker offers. */
  readonly sorted: string[];
  /** Which module `file` belongs to, or `undefined` on a single-module project (or for a file
   *  that lives outside every module — the root `pom.xml`, a hit inside a dependency jar). */
  moduleOf(file: string): string | undefined;
  /** `file`, relative to the project root. */
  relative(file: string): string;
}

export function moduleIndex(project: ProjectInfo | null | undefined): ModuleIndex {
  const modules = moduleList(project);
  const root = project?.root;
  return {
    modules,
    sorted: [...modules].sort((a, b) => a.localeCompare(b)),
    relative: (file) => relativeTo(root, file),
    moduleOf(file) {
      const rel = relativeTo(root, file);
      return modules.find((m) => rel === m || rel.startsWith(`${m}/`));
    },
  };
}
