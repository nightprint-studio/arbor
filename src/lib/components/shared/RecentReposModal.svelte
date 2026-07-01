<script lang="ts">
  /**
   * RecentReposModal — a keyboard-first picker over the recent-repository list.
   *
   * Chrome opened from the app titlebar's hamburger menu, so it lives in the
   * top-level `shared/` tier. It filters `uiStore.recentRepos` (an array of
   * absolute paths), and picking a row opens the repo through the exact same
   * `open-recent` document event the hamburger menu used to dispatch inline —
   * AppShell owns the open flow on the other end.
   */
  import { Clock, Search } from 'lucide-svelte';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Input from './ui/Input.svelte';
  import EmptyState from './ui/EmptyState.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let query      = $state('');
  let selectedIdx = $state(0);
  let listEl     = $state<HTMLElement | undefined>();

  /** Last path segment for a recent-repo label. */
  function basename(path: string): string {
    return path.split(/[/\\]/).filter(Boolean).pop() ?? path;
  }

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const all = uiStore.recentRepos;
    if (!q) return all;
    return all.filter(
      (p) => p.toLowerCase().includes(q) || basename(p).toLowerCase().includes(q),
    );
  });

  // Keep the selection in range as the filtered list shrinks.
  $effect(() => {
    if (selectedIdx >= filtered.length) selectedIdx = Math.max(0, filtered.length - 1);
  });

  // Reset the cursor to the top whenever the query changes.
  let lastQuery = '';
  $effect(() => {
    if (query !== lastQuery) {
      lastQuery = query;
      selectedIdx = 0;
    }
  });

  function scrollIntoView() {
    requestAnimationFrame(() => {
      listEl?.querySelector<HTMLElement>(`[data-idx="${selectedIdx}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    });
  }

  function open(path: string) {
    // Dispatched EXACTLY as the hamburger menu did — AppShell listens for this.
    document.dispatchEvent(new CustomEvent('open-recent', { detail: path, bubbles: true }));
    onClose();
  }

  function onInputKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, filtered.length - 1);
      scrollIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      scrollIntoView();
    } else if (e.key === 'Enter') {
      const path = filtered[selectedIdx];
      if (path) { e.preventDefault(); open(path); }
    }
    // Esc bubbles up to <Modal>'s window handler, which closes the topmost modal.
  }
</script>

<Modal {onClose} width="560px" height="520px" padBody={false} ariaLabel="Recent repositories">
  {#snippet header()}
    <ModalHeader title="Recent repositories" {onClose} />
  {/snippet}

  {#snippet children()}
    <div class="rr-body">
      <div class="rr-search">
        <Input
          bind:value={query}
          placeholder="Filter recent repositories…"
          ariaLabel="Filter recent repositories"
          onkeydown={onInputKeydown}
          size="md"
          autofocus
        >
          {#snippet iconStart()}<Search size={14} />{/snippet}
        </Input>
      </div>

      {#if uiStore.recentRepos.length === 0}
        <EmptyState message="No recent repositories yet." />
      {:else if filtered.length === 0}
        <EmptyState message="No repositories match your filter." />
      {:else}
        <div class="rr-list" bind:this={listEl} role="listbox" tabindex="-1" aria-label="Recent repositories">
          {#each filtered as path, idx (path)}
            <button
              type="button"
              class="rr-item"
              class:selected={idx === selectedIdx}
              data-idx={idx}
              role="option"
              aria-selected={idx === selectedIdx}
              onmouseenter={() => { selectedIdx = idx; }}
              onclick={() => open(path)}
            >
              <span class="rr-icon"><Clock size={15} /></span>
              <span class="rr-text">
                <span class="rr-name">{basename(path)}</span>
                <span class="rr-path">{path}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/snippet}
</Modal>

<style>
  .rr-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .rr-search {
    padding: 12px 12px 8px;
    flex-shrink: 0;
  }

  .rr-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .rr-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .rr-item:hover { background: var(--bg-hover); }
  .rr-item.selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    outline: none;
  }

  .rr-icon {
    display: flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .rr-item.selected .rr-icon { color: var(--accent); }

  .rr-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .rr-name {
    font-size: var(--font-size-sm);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rr-path {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
</style>
