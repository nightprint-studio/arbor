<script lang="ts">
  /**
   * Rename the open project — writes the root `name` in `merula.toml` (the rest
   * of the manifest is preserved). Keyboard-first: the field auto-focuses, Enter
   * or Ctrl+Enter submits, Esc cancels (handled by Modal).
   */
  import { FolderPen } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { projectStore } from '../stores/project.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let name = $state(projectStore.project?.name ?? '');
  let busy = $state(false);

  const canSave = $derived(name.trim().length > 0 && name.trim() !== projectStore.project?.name);

  async function save() {
    if (!canSave || busy) return;
    busy = true;
    try {
      await projectStore.rename(name);
      onClose();
    } finally {
      busy = false;
    }
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); void save(); }
  }
</script>

<Modal {onClose} width="520px" height="240px" ariaLabel="Rename project">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderPen size={14} />
      <span class="modal-title">Rename project</span>
    </ModalHeader>
  {/snippet}

  <div class="rp">
    <FormField label="Project name" hint="Stored as `name` in merula.toml.">
      <Input bind:value={name} autofocus placeholder="My Song" onkeydown={onKeydown} ariaLabel="Project name" />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSave || busy} onclick={save}>Rename</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .rp { padding: 16px 4px 4px; }
</style>
