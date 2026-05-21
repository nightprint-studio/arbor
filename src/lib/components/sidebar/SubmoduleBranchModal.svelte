<script lang="ts">
  import { GitBranch, Check } from 'lucide-svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { SubmoduleInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { submoduleListBranches, submoduleCheckout } from '$lib/ipc/submodule';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    sub,
    onClose,
    onDone,
  }: {
    sub: SubmoduleInfo;
    onClose: () => void;
    onDone: () => void;
  } = $props();

  const tab = $derived(tabsStore.activeTab);

  let branches    = $state<string[]>([]);
  // svelte-ignore state_referenced_locally
  let selected    = $state<string>(sub.branch ?? '');
  let loadingList = $state(true);
  let checking    = $state(false);
  let listError   = $state<string | null>(null);

  $effect(() => {
    if (!tab) return;
    submoduleListBranches(tab.id, sub.path)
      .then((b) => { branches = b; loadingList = false; })
      .catch((err) => { listError = String(err); loadingList = false; });
  });

  async function handleConfirm() {
    if (!tab || !selected || checking) return;
    checking = true;
    try {
      await submoduleCheckout(tab.id, sub.path, selected);
      uiStore.showToast(`Checked out "${selected}" in "${sub.name}"`, 'success');
      onDone();
    } catch (err) {
      uiStore.showToast(`Checkout failed: ${err}`, 'error');
      checking = false;
    }
  }

  function handleEnter(e: KeyboardEvent) {
    if (e.key === 'Enter' && selected) handleConfirm();
  }
</script>

<svelte:window onkeydown={handleEnter} />

<Modal {onClose} ariaLabel="Checkout Branch">
  {#snippet header()}
    <ModalHeader {onClose}>
      <span class="header-icon"><GitBranch size={14} /></span>
      <span class="modal-title">Checkout Branch</span>
      <span class="header-sub">{sub.name}</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if loadingList}
      <div class="loading-state">
        <Spinner size={16} />
        <span>Loading branches…</span>
      </div>
    {:else if listError}
      <div class="error-state">{listError}</div>
    {:else if branches.length === 0}
      <div class="empty-state">No branches found.</div>
    {:else}
      <div class="branch-list" role="listbox" aria-label="Branches">
        {#each branches as b (b)}
          {@const isCurrent = b === sub.branch}
          <button
            class="branch-item"
            class:current={isCurrent}
            class:selected={selected === b}
            role="option"
            aria-selected={selected === b}
            onclick={() => selected = b}
            ondblclick={handleConfirm}
            use:tooltip={isCurrent ? `${b} (current)` : b}
          >
            <span class="branch-icon"><GitBranch size={11} /></span>
            <span class="branch-name">{b}</span>
            {#if isCurrent}
              <span class="current-pill">current</span>
            {/if}
            {#if selected === b && !isCurrent}
              <span class="check-icon"><Check size={11} /></span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={onClose} disabled={checking}>Cancel</Button>
    <Button
      variant="primary"
      onclick={handleConfirm}
      disabled={!selected || checking || selected === sub.branch}
      loading={checking}
    >
      {#snippet iconStart()}
        <Check size={12} />
      {/snippet}
      {checking ? 'Checking out…' : 'Checkout'}
    </Button>
  {/snippet}
</Modal>

<style>
  .header-icon {
    display: flex;
    color: var(--accent);
    flex-shrink: 0;
  }

  .header-sub {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .body {
    padding: 0;
    margin: -16px;
  }

  .loading-state,
  .error-state,
  .empty-state {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 16px;
    font-size: 12px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  .error-state { color: var(--error, #c75450); }

  .branch-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px;
  }

  .branch-item {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-secondary);
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .branch-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .branch-item.current { color: var(--text-muted); }
  .branch-item.selected {
    background: var(--accent-subtle);
    border-color: rgba(77,120,204,0.30);
    color: var(--text-primary);
  }

  .branch-icon {
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    flex-shrink: 0;
  }
  .branch-item.selected .branch-icon { color: var(--accent); }

  .branch-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .current-pill {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--text-muted);
    background: rgba(255,255,255,0.06);
    border: 1px solid rgba(255,255,255,0.10);
    padding: 0 5px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  .check-icon {
    display: flex;
    align-items: center;
    color: var(--accent);
    flex-shrink: 0;
  }

</style>
