<script lang="ts">
  /**
   * Rename a `.merula` file in place (same folder). Keyboard-first: the field
   * auto-focuses with the base name pre-selected, Enter / Ctrl+Enter submits, Esc
   * cancels (handled by Modal). The `.merula` extension is optional in the input —
   * the store appends it when missing.
   */
  import { untrack } from 'svelte';
  import { FilePen } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { projectStore } from '../stores/project.svelte';

  let { path, currentName, onClose }: { path: string; currentName: string; onClose: () => void } =
    $props();

  // Seed the editable field once from the prop (a one-time snapshot, not a live
  // mirror) — `untrack` makes that intent explicit and silences the warning.
  let name = $state(untrack(() => currentName));
  let busy = $state(false);

  const canSave = $derived(name.trim().length > 0 && !/[\\/]/.test(name) && name.trim() !== currentName);

  async function save() {
    if (!canSave || busy) return;
    busy = true;
    try {
      await projectStore.renameFile(path, name);
      onClose();
    } finally {
      busy = false;
    }
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<Modal {onClose} width="520px" height="240px" ariaLabel="Rename file">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FilePen size={14} />
      <span class="modal-title">Rename file</span>
    </ModalHeader>
  {/snippet}

  <div class="rf">
    <FormField label="File name" hint="The .merula extension is added automatically.">
      <Input bind:value={name} autofocus placeholder="song.merula" onkeydown={onKeydown} ariaLabel="File name" />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSave || busy} onclick={save}>Rename</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .rf { padding: 16px 4px 4px; }
</style>
