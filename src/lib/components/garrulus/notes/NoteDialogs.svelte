<script lang="ts">
  /**
   * Every dialog the note lifecycle opens, rendered from the ui store's single
   * `noteDialog` slot (plus the quick switcher's own flag).
   *
   * Here rather than in the components that open them for the reason
   * `GarrulusOverlays` gives: each has several ways in (a chord, a palette verb,
   * a tree context menu) and none of them should have to own the dialog.
   */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import NewNoteModal from './NewNoteModal.svelte';
  import QuickSwitcherModal from './QuickSwitcherModal.svelte';
  import RenameNoteModal from './RenameNoteModal.svelte';
  import TypePickerModal from './TypePickerModal.svelte';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import { garrulusUiStore } from '$lib/stores/garrulus/ui.svelte';

  const dialog = $derived(garrulusUiStore.noteDialog);

  const deleteTitle = $derived(
    dialog?.kind === 'delete'
      ? (garrulusNotesStore.notes.find((n) => n.path === dialog.path)?.title ?? dialog.path)
      : '',
  );

  async function confirmDelete(path: string) {
    garrulusUiStore.closeNoteDialog();
    await garrulusNotesStore.remove(path);
  }

  async function pickType(typeId: string, apply: string | null) {
    if (apply) {
      garrulusUiStore.closeNoteDialog();
      await garrulusNotesStore.applyType(apply, typeId);
    } else {
      garrulusUiStore.openNoteDialog({ kind: 'new', typeId });
    }
  }
</script>

{#if garrulusUiStore.switcherOpen}
  <QuickSwitcherModal onClose={() => garrulusUiStore.closeSwitcher()} />
{/if}

{#if dialog?.kind === 'new'}
  <NewNoteModal typeId={dialog.typeId} onClose={() => garrulusUiStore.closeNoteDialog()} />
{:else if dialog?.kind === 'pick-type'}
  <TypePickerModal
    placeholder={dialog.apply ? 'Give this note which type…' : 'New note of which type…'}
    onPick={(id) => void pickType(id, dialog.apply)}
    onClose={() => garrulusUiStore.closeNoteDialog()}
  />
{:else if dialog?.kind === 'rename'}
  <RenameNoteModal path={dialog.path} onClose={() => garrulusUiStore.closeNoteDialog()} />
{:else if dialog?.kind === 'delete'}
  <ConfirmModal
    title="Delete note"
    message={`Move “${deleteTitle}” to the vault trash?`}
    detail="It can be restored from the trash without going through git."
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => void confirmDelete(dialog.path)}
    onCancel={() => garrulusUiStore.closeNoteDialog()}
  />
{/if}
