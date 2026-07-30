<script lang="ts">
  /**
   * Picus keyboard reference — read-only, sourced from `PICUS_SHORTCUTS`.
   *
   * Window-local bindings, like Tyto's: Picus is a standalone product window, so
   * its shortcuts don't live in the global (Corvus) keybindings store and are
   * not rebindable from here.
   */
  import { Keyboard } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ShortcutsReference from '$lib/components/shared/ui/ShortcutsReference.svelte';
  import { PICUS_SHORTCUTS } from './picus-shortcuts';

  let { onClose }: { onClose: () => void } = $props();
</script>

<Modal {onClose} width="660px" height="580px" padBody={false} ariaLabel="Picus keyboard shortcuts">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Keyboard size={14} />
      <span class="modal-title">Keyboard Shortcuts</span>
    </ModalHeader>
  {/snippet}

  <ShortcutsReference groups={PICUS_SHORTCUTS} />
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
</style>
