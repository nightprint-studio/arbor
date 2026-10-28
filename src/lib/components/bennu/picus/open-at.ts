/** Open `file` in the editor and put the caret on `line` — the one way the Picus tool window jumps
 *  to source, so an entity row and a query row cannot disagree about how. */

import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

export function openAt(file: string, line: number): void {
  void projectStore.openFile(file).then(() => {
    if (line) bennuUiStore.requestGoto(line);
  });
}
