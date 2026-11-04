<script lang="ts">
  /**
   * F2 — rename or move a note.
   *
   * One field holding the whole vault-relative path, with the file stem selected:
   * typing replaces the name, `Enter` takes it, and a slash moves the note to
   * another folder with the same gesture.
   *
   * Links that pointed at the old name are not rewritten; the Problems panel lists
   * them as unresolved. The dialog says so, because the surprise is worse than the
   * sentence.
   */
  import { Pencil } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';
  import {
    normalizeNotePath,
    notePathProblem,
    stemRange,
    withNoteExtension,
  } from './note-naming';

  interface Props {
    /** Vault-relative path of the note being renamed. */
    path: string;
    onClose: () => void;
  }

  let { path, onClose }: Props = $props();

  let value = $state(path);
  let field = $state<HTMLInputElement | undefined>();
  let busy = $state(false);

  const taken = $derived(
    new Set(garrulusNotesStore.notes.flatMap((n) => (n.path ? [n.path] : []))),
  );
  const target = $derived(withNoteExtension(normalizeNotePath(value)));
  const unchanged = $derived(target === path);
  const problem = $derived(unchanged ? null : notePathProblem(value, taken, path));
  const canSubmit = $derived(!unchanged && !problem && !busy);

  // Focus and select the stem once the field exists (no `autofocus`, a11y).
  $effect(() => {
    if (!field) return;
    field.focus();
    const [from, to] = stemRange(path);
    field.setSelectionRange(from, to);
  });

  async function submit() {
    if (!canSubmit) return;
    busy = true;
    try {
      if (await garrulusNotesStore.rename(path, target)) onClose();
    } finally {
      busy = false;
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    void submit();
  }
</script>

<Modal {onClose} width="560px" height="330px" ariaLabel="Rename note">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Pencil size={14} />
      <span class="modal-title">Rename note</span>
    </ModalHeader>
  {/snippet}

  <div class="grn-body">
    <FormField label="Path" error={problem} hint="Include a folder to move the note there.">
      <Input bind:value bind:element={field} onkeydown={onKeyDown} />
    </FormField>
    <Alert variant="info">
      Links to this note are not rewritten — any that no longer resolve appear under Problems.
    </Alert>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSubmit} onclick={submit}>Rename</Button>
  {/snippet}
</Modal>

<style>
  .grn-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
