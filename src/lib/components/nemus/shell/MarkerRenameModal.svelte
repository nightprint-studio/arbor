<script lang="ts">
  /**
   * Rename a ruler marker. Keyboard-first: the field auto-focuses, Enter / Ctrl+
   * Enter submits, Esc cancels (handled by Modal). Mirrors RenameProjectModal.
   */
  import { MapPin } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { transportUiStore, type Marker } from '../stores/transport-ui.svelte';

  let { marker, onClose }: { marker: Marker; onClose: () => void } = $props();

  let label = $state(marker.label);
  const canSave = $derived(label.trim().length > 0);

  function save() {
    if (!canSave) return;
    transportUiStore.renameMarker(marker.id, label.trim());
    onClose();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); save(); }
  }
</script>

<Modal {onClose} width="520px" height="240px" ariaLabel="Rename marker">
  {#snippet header()}
    <ModalHeader {onClose}>
      <MapPin size={14} />
      <span class="modal-title">Rename marker</span>
    </ModalHeader>
  {/snippet}

  <div class="mr">
    <FormField label="Marker name">
      <Input bind:value={label} autofocus placeholder="Chorus" onkeydown={onKeydown} ariaLabel="Marker name" />
    </FormField>
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <Button variant="primary" disabled={!canSave} onclick={save}>Rename</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .mr { padding: 16px 4px 4px; }
</style>
