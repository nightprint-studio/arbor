/**
 * What the assistant can open for the user, and how.
 *
 * The backend names a target ("vulnerabilities", "problems", …) from a fixed list; this maps each name to
 * the call that opens the window or panel. Kept apart from the store so it is a plain function over a small
 * interface, and tested without the editor around it.
 */

/** The part of the editor's interface state this needs. */
export interface UiHost {
  showBottom(panel: string): void;
  showRight(panel: string): void;
  showLeft(panel: string): void;
  openVulnerabilities(): void;
  openModuleGraph(): void;
  openSsr(): void;
  openFind(initial?: string): void;
  openSettings(): void;
}

const OPENERS: Record<string, (ui: UiHost, query?: string) => void> = {
  vulnerabilities: (ui) => ui.openVulnerabilities(),
  problems: (ui) => ui.showBottom('problems'),
  tests: (ui) => ui.showRight('tests'),
  run: (ui) => ui.showBottom('run'),
  terminal: (ui) => ui.showBottom('terminal'),
  git_changes: (ui) => ui.showBottom('changes'),
  todos: (ui) => ui.showBottom('todos'),
  dependencies: (ui) => ui.showLeft('dependencies'),
  module_graph: (ui) => ui.openModuleGraph(),
  structural_search: (ui) => ui.openSsr(),
  find_in_files: (ui, query) => ui.openFind(query ?? ''),
  settings: (ui) => ui.openSettings(),
};

/** Open `target`. `false` when this build does not know it, so the caller can say so. */
export function openUi(ui: UiHost, target: string, query?: string): boolean {
  const open = OPENERS[target];
  if (!open) return false;
  open(ui, query);
  return true;
}
