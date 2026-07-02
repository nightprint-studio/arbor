<script lang="ts">
  /**
   * Services / Run (right tool window) — a MOCK run/services panel. No run
   * configurations exist yet (build/run isn't implemented), so this is a clearly
   * marked sketch showing the empty state + a disabled "Add configuration"
   * affordance. Reuses shared/ui only.
   */
  import { Server, Plus } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
</script>

<PanelShell title="Services">
  {#snippet icon()}<Server size={13} />{/snippet}
  {#snippet actions()}
    <button
      class="ps-btn"
      type="button"
      use:tooltip={'Add run configuration'}
      aria-label="Add run configuration"
      onclick={() => toastStore.show("Run configurations aren't implemented yet.", 'info')}
    >
      <Plus size={14} />
    </button>
  {/snippet}

  <div class="sv-mock">Sketch — run/services aren't wired yet.</div>
  <div class="sv-empty">
    <EmptyState message="No run configurations." />
  </div>
</PanelShell>

<style>
  .sv-mock {
    padding: 6px 12px;
    font-size: 10.5px; font-style: italic;
    color: var(--text-disabled);
    border-bottom: 1px solid var(--border-subtle);
  }
  .sv-empty {
    flex: 1; display: flex; align-items: center; justify-content: center;
    min-height: 0; padding: 20px 12px;
  }
</style>
