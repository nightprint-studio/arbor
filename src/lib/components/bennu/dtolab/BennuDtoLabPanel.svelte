<script lang="ts">
  /**
   * The DTO Lab — a class tried out as JSON (Payload) and turned into tests (Tests), in the bottom dock.
   *
   * Everything it shows lives in `bennuDtoLabStore`: the panel is mounted only while it is visible,
   * and a payload somebody spent a minute writing must survive a look at the code.
   */
  import { Beaker, RefreshCw } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuDtoLabStore as lab, type DtoLabTab } from '$lib/stores/bennu/dtolab.svelte';
  import DtoLabPayload from './DtoLabPayload.svelte';
  import DtoLabTests from './DtoLabTests.svelte';
  import DtoLabPreviewModal from './DtoLabPreviewModal.svelte';

  const constrained = $derived(lab.view?.class.fields.filter((f) => f.constraints.length > 0).length ?? 0);
  const tabs = $derived([
    { id: 'payload', label: 'Payload' },
    { id: 'tests', label: 'Tests', badge: constrained > 0 ? constrained : undefined },
  ]);
</script>

<div class="lab">
  <BottomPanelHeader title="DTO Lab" onClose={() => bennuUiStore.closeBottom()}>
    {#snippet icon()}<Beaker size={13} />{/snippet}
    {#if lab.view}
      <span class="lab-class" use:tooltip={lab.view.class.fqn}>{lab.view.class.name}</span>
      <Tabs
        items={tabs}
        value={lab.tab}
        variant="pill"
        size="sm"
        ariaLabel="DTO Lab views"
        onSelect={(id) => lab.setTab(id as DtoLabTab)}
      />
    {/if}
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Read the class again'}
        aria-label="Read the class again"
        disabled={!lab.origin || lab.opening}
        onclick={() => lab.reload()}
      >
        <RefreshCw size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  {#if lab.opening}
    <div class="state"><Spinner size={13} /> Reading the class…</div>
  {:else if lab.openError}
    <div class="empty"><EmptyState message={lab.openError} description="Put the caret inside a Java class and press Alt+Shift+J." /></div>
  {:else if !lab.view}
    <div class="empty">
      <Beaker size={20} />
      <EmptyState
        message="No class yet"
        description="Put the caret inside a Java class and press Alt+Shift+J, or choose Open in DTO Lab from the editor's menu."
      />
    </div>
  {:else if lab.tab === 'payload'}
    <DtoLabPayload />
  {:else}
    <DtoLabTests />
  {/if}
</div>

{#if lab.preview}
  <DtoLabPreviewModal />
{/if}

<style>
  .lab { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
  .lab-class {
    margin: 0 8px 0 4px;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
    white-space: nowrap;
  }
  .state { display: flex; align-items: center; gap: 7px; padding: 12px 14px; font-size: var(--font-size-sm); color: var(--text-secondary); }
  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; color: var(--text-disabled); }
</style>
