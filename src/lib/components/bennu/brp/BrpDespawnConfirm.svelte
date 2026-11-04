<script lang="ts">
  /** The question before an entity is despawned from the running game — one place, asked from the tree and from the detail. */
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { bennuBrpStore as brp } from '$lib/stores/bennu/brp.svelte';
  import { entityTag } from './brp-value';

  let { id, onClose }: { id: number | null; onClose: () => void } = $props();

  async function confirm() {
    const target = id;
    onClose();
    if (target !== null) await brp.despawn(target);
  }
</script>

{#if id !== null}
  <ConfirmModal
    title="Despawn entity"
    message={`Despawn ${entityTag(id)} from the running game?`}
    detail="Its children go with it. The game is not restarted, and this cannot be undone."
    variant="danger"
    confirmLabel="Despawn"
    onConfirm={() => void confirm()}
    onCancel={onClose}
  />
{/if}
