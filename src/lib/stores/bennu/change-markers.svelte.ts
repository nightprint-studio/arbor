/**
 * The change gutter's data: which lines of the open buffer differ from HEAD.
 *
 * Asked of Corvus a moment after typing stops — never per keystroke — and not at all when the
 * setting is off or the project is not in a repository, so a user who does not want the extra
 * calls makes none. The answer belongs to the file it was computed for; a stale one (the user
 * switched tabs, or typed on) is dropped.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */
import { getChangeMarkers } from '$lib/ipc/corvus/changeMarkers';
import type { ChangeHunk } from '$lib/components/shared/ui/code-editor/change-gutter';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
import { bennuGitStatusStore } from '$lib/stores/bennu/git-status.svelte';

/** Quiet period after the last edit before the buffer is diffed. */
const IDLE_MS = 300;

function createChangeMarkersStore() {
  // Replaced wholesale, never mutated.
  let hunks = $state.raw<ChangeHunk[]>([]);
  let forPath = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let token = 0;

  function reset() {
    if (timer) clearTimeout(timer);
    token++;
    hunks = [];
    forPath = null;
  }

  return {
    /** The hunks to draw for `path` — empty for any other file. */
    hunksFor(path: string | null): ChangeHunk[] {
      return path && forPath === path ? hunks : [];
    },

    /** The buffer of `path` is now `text` (or the file/commit changed): re-diff after a pause. */
    request(path: string | null, text: string) {
      if (timer) clearTimeout(timer);
      if (!path || !bennuSettingsStore.showChangeMarkers || !bennuGitStatusStore.inRepo) {
        reset();
        return;
      }
      timer = setTimeout(async () => {
        const mine = ++token;
        try {
          const r = await getChangeMarkers(path, text);
          if (mine !== token) return;
          hunks = r.tracked && !r.skipped ? r.hunks : [];
          forPath = path;
        } catch {
          if (mine !== token) return;
          hunks = [];
          forPath = path;
        }
      }, IDLE_MS);
    },

    reset,
  };
}

export const bennuChangeMarkersStore = createChangeMarkersStore();
