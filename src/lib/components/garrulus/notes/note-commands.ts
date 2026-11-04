/**
 * The note verbs, each in one place.
 *
 * A verb has two ways in — its chord in the shell and its entry in the palette —
 * and they must do the same thing. Both call these, so "New note" cannot mean one
 * dialog from the keyboard and another from the menu.
 *
 * Every one only *opens* something (a dialog, a note) or asks the store to; none
 * writes on its own authority. The writes are the dialogs' confirm buttons.
 */

import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';
import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';

/** The vault's daily-note type: `daily` unless the vault declares otherwise.
 *  (`vault.toml`'s `[daily] type_id` has no wrapper yet — the default is what every
 *  vault created by Garrulus carries.) */
const DAILY_TYPE = 'daily';

export const noteCommands = {
  quickSwitch: () => garrulusUiStore.openSwitcher(),

  newNote: () => garrulusUiStore.openNoteDialog({ kind: 'new', typeId: null }),

  /** Straight to the form when the type is known, via the picker when it is not. */
  newTypedNote: (typeId?: string) => {
    if (typeId) garrulusUiStore.openNoteDialog({ kind: 'new', typeId });
    // A vault with no types has nothing to pick: the plain form is the honest answer.
    else if (garrulusVaultStore.types.length === 0) noteCommands.newNote();
    else garrulusUiStore.openNoteDialog({ kind: 'pick-type', apply: null });
  },

  renameActive: () => {
    const path = garrulusNotesStore.activePath;
    if (path) garrulusUiStore.openNoteDialog({ kind: 'rename', path });
  },

  deleteActive: () => {
    const path = garrulusNotesStore.activePath;
    if (path) garrulusUiStore.openNoteDialog({ kind: 'delete', path });
  },

  /** "Promote to Bug": the picker, then the type is applied to the open note. */
  applyTypeToActive: (typeId?: string) => {
    const path = garrulusNotesStore.activePath;
    if (!path) return;
    if (typeId) void garrulusNotesStore.applyType(path, typeId);
    else garrulusUiStore.openNoteDialog({ kind: 'pick-type', apply: path });
  },

  saveActive: () => void garrulusNotesStore.save(),

  openDaily: () =>
    void garrulusNotesStore.openDaily(
      DAILY_TYPE,
      garrulusVaultStore.types.some((t) => t.id === DAILY_TYPE),
    ),
};
