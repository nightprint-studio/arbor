/**
 * What clicking a piece of an interpreted log line means, in Bennu.
 *
 * The interpreter (`arbor-logscan`, on the backend) says *that* a piece points somewhere;
 * where it should land is a property of the host, so it lives here and not in the shared
 * widget. One place, because the Run console, the Build log and the Test log ask the same
 * question.
 *
 * Three kinds arrive, and the third is the interesting one:
 *
 * * a **URL** opens in the browser;
 * * a **file** opens in the editor at its line — a stack frame in a class this project
 *   declares arrives already resolved, because the backend had the class index in memory as
 *   the line went past;
 * * a **source** is a frame in the JDK or a dependency, left unresolved on purpose. Finding
 *   the source of `java.lang.Thread` means reading jars, and that is worth paying for the one
 *   frame someone clicks rather than for all forty of them as they stream. So it is asked
 *   here, once, on the click.
 */

import { openUrl } from '@tauri-apps/plugin-opener';
import type { LogLink } from '$lib/types/log';
import { frameSource } from '$lib/ipc/bennu/nav';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
import { decompiledStore } from '$lib/stores/bennu/decompiled.svelte';

/** Backslashes to forward slashes — the form every path in the project store is in. */
function canon(path: string): string {
  return path.replace(/\\/g, '/');
}

/**
 * Follow a log link: a URL opens in the browser, a file opens in the editor at its line, a
 * library frame opens that class's source view at the method it named.
 *
 * Best-effort by design. A path scraped out of a log can be stale, relative to a working
 * directory we do not have, or simply not a file at all; a frame can name a class no artifact
 * on this machine holds. None of that is worth an error dialog over a console click.
 */
export async function openLogLink(link: LogLink): Promise<void> {
  try {
    if (link.kind === 'url') {
      await openUrl(link.url);
      return;
    }
    if (link.kind === 'file') {
      await projectStore.openFile(canon(link.path));
      // After the open, so the editor exists to be scrolled.
      if (link.line) bennuUiStore.requestGoto(link.line);
      return;
    }
    await openLibraryClass(link.class, link.method, link.line);
  } catch {
    /* a link that cannot be followed is not an error — see above */
  }
}

/**
 * Open a class that lives in a **library or the JDK**: resolve its source view now, then show
 * what the backend produced.
 *
 * Exported because a stack frame is not the only way to arrive at one — the Go-to navigator
 * offers the classpath's classes directly, and "what happens when you open a library class"
 * must have one answer. It is a question with several parts (real source or a stub? where in
 * it? can the sources be downloaded?) and two implementations of it would drift on all three.
 *
 * The **offset** is where to land, and the backend decided it knowing something this side
 * cannot: whether it served real source (the frame's line is a fact) or a decompiled stub
 * (the line numbers are fiction, so it lands on the method instead). Registering the view
 * with {@link decompiledStore} is what puts the "Download sources" banner on the tab when a
 * stub was served — after which the real lines are one click away.
 */
export async function openLibraryClass(
  fqcn: string,
  method?: string,
  line?: number,
): Promise<void> {
  const root = projectStore.project?.root;
  if (!root) return;
  const view = await frameSource(root, fqcn, method, line);
  if (!view) return;
  decompiledStore.register(view.file, {
    // The project root stands in for "a file inside the project", which is all the download
    // needs it for — a root is a path under itself.
    originFile: root,
    originSource: '',
    name: fqcn,
    canDownload: view.can_download,
  });
  await projectStore.openFile(view.file);
  if (view.offset > 0) bennuUiStore.requestGotoOffset(view.offset);
  else bennuUiStore.requestGoto(1);
}
