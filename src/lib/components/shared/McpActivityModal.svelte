<!--
  The call log as a window of its own, reachable from every product.

  A frame, not a second implementation: `McpActivity` is the panel, and the AI settings
  page shows the same one. It is here as well because "what is that assistant doing right
  now" is asked from wherever you happen to be working, and the settings modal lives on
  the home surface — which, in tabbed mode, is closed for as long as a product is open.
-->
<script lang="ts">
  import { Activity } from 'lucide-svelte';

  import McpActivity from '$lib/components/shared/McpActivity.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { mcpStore } from '$lib/stores/mcp.svelte';

  let { onClose }: { onClose: () => void } = $props();

  // Per-window store: without this the panel would render an empty log as if it were the
  // record, in every window that is not the home surface.
  $effect(() => { void mcpStore.ensureLoaded(); });

  let error = $state<string | null>(null);

  async function guard(action: () => Promise<void>) {
    error = null;
    try {
      await action();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<Modal {onClose} size="lg" width="760px" height="620px" ariaLabel="AI activity">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Activity size={14} />
      <span class="modal-title">AI activity</span>
    </ModalHeader>
  {/snippet}

  <McpActivity {guard} fill />

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">
        {#if error}{error}{:else}Kept in memory only — a record of this run, not a file growing on disk.{/if}
      </span>
      <Button variant="primary" onclick={onClose}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .hint { font-size: 11px; line-height: 1.5; color: var(--text-tertiary); max-width: 62ch; }
</style>
