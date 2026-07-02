/**
 * Tyto UI store — window-chrome state shared across the shell, titlebar and
 * footer: the overlay panels (docs / settings / shortcuts / about) opened from
 * the titlebar gear + hamburger, and the right-rail captures library toggle.
 *
 * Kept separate from `recorderStore` (the capture domain) so chrome state and
 * domain state don't tangle. Mirrors the shape Merula uses on its own store.
 */
import { setTytoCompact } from '$lib/ipc/tyto/main-window';

function createTytoUiStore() {
  let docsOpen = $state(false);
  let settingsOpen = $state(false);
  let shortcutsOpen = $state(false);
  let aboutOpen = $state(false);

  // Right-rail captures library (collapsible). Ephemeral session state — not a
  // persisted setting, so it lives here, not in config.toml.
  let libraryOpen = $state(true);

  // Compact "mini" presentation of the whole window (a Snip-like quick-capture
  // toolbar) vs the full control panel. Ephemeral; the shell resizes the OS window
  // to match. After a capture the shell drops back to full (see TytoShell).
  let compact = $state(false);
  function applyCompact(v: boolean) {
    compact = v;
    void setTytoCompact(v).catch(() => {});
  }

  return {
    get docsOpen() { return docsOpen; },
    get settingsOpen() { return settingsOpen; },
    get shortcutsOpen() { return shortcutsOpen; },
    get aboutOpen() { return aboutOpen; },
    get libraryOpen() { return libraryOpen; },
    get compact() { return compact; },

    /** True while any modal/overlay owns the screen — used to gate window
     *  shortcuts so they don't fire behind an open dialog. */
    get anyModalOpen() { return docsOpen || settingsOpen || shortcutsOpen || aboutOpen; },

    /** Enter/leave the compact mini toolbar (resizes the OS window via the shell). */
    setCompact(v: boolean) { applyCompact(v); },
    toggleCompact() { applyCompact(!compact); },

    openDocs() { docsOpen = true; },
    closeDocs() { docsOpen = false; },
    toggleDocs() { docsOpen = !docsOpen; },

    openSettings() { settingsOpen = true; },
    closeSettings() { settingsOpen = false; },

    openShortcuts() { shortcutsOpen = true; },
    closeShortcuts() { shortcutsOpen = false; },

    openAbout() { aboutOpen = true; },
    closeAbout() { aboutOpen = false; },

    toggleLibrary() { libraryOpen = !libraryOpen; },
    setLibraryOpen(v: boolean) { libraryOpen = v; },
  };
}

export const tytoUiStore = createTytoUiStore();
