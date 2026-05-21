<script lang="ts">
  import { FolderOpen, Search, Clock } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import Modal from './Modal.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onOpen }: { onOpen: (path: string) => void } = $props();

  let query   = $state('');
  let cursor  = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  const allRecent  = $derived(uiStore.recentRepos);
  const filtered   = $derived(
    query.trim()
      ? allRecent.filter(p => p.toLowerCase().includes(query.toLowerCase()))
      : allRecent
  );

  $effect(() => {
    void filtered;
    cursor = 0;
  });

  $effect(() => {
    if (uiStore.recentQuickSwitchOpen) {
      query  = '';
      cursor = 0;
      requestAnimationFrame(() => inputEl?.focus());
    }
  });

  function close() { uiStore.setRecentQuickSwitchOpen(false); }

  function pick(path: string) {
    close();
    onOpen(path);
  }

  function onKeydown(e: KeyboardEvent) {
    if (!uiStore.recentQuickSwitchOpen) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cursor = Math.min(cursor + 1, filtered.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      cursor = Math.max(cursor - 1, 0);
    } else if (e.key === 'Enter') {
      if (filtered[cursor]) pick(filtered[cursor]);
    }
  }

  function repoName(path: string): string {
    return path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? path;
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if uiStore.recentQuickSwitchOpen}
  <Modal onClose={close} size="md" padBody={false} ariaLabel="Recent repositories">
    {#snippet header()}
      <Search size={14} class="search-icon" />
      <input
        bind:this={inputEl}
        bind:value={query}
        class="search-input"
        placeholder="Filter recent repositories…"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="mac-close-btn" onclick={close} use:tooltip={'Close'} aria-label="Close"></button>
    {/snippet}

    <div class="list">
      {#if filtered.length === 0}
        <div class="empty">
          {#if query}
            No repositories match <em>{query}</em>
          {:else}
            No recent repositories
          {/if}
        </div>
      {:else}
        {#each filtered as path, i}
          <button
            class="item"
            class:active={i === cursor}
            onclick={() => pick(path)}
            onmouseenter={() => (cursor = i)}
          >
            <span class="item-icon"><Clock size={12} /></span>
            <span class="item-text">
              <span class="item-name">{repoName(path)}</span>
              <span class="item-path">{path}</span>
            </span>
            <span class="item-open"><FolderOpen size={12} /></span>
          </button>
        {/each}
      {/if}
    </div>

    {#snippet footer()}
      <span class="hint"><kbd>↑↓</kbd> navigate · <kbd>Enter</kbd> open · <kbd>Esc</kbd> close</span>
    {/snippet}
  </Modal>
{/if}

<style>
  :global(.search-icon) { color: var(--text-muted); flex-shrink: 0; }

  .search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 13px;
  }

  .search-input::placeholder { color: var(--text-disabled); }

  .list {
    max-height: 360px;
    overflow-y: auto;
    padding: 4px 0;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }

  .empty {
    padding: 24px;
    text-align: center;
    color: var(--text-disabled);
    font-size: 12px;
  }
  .empty em { color: var(--text-muted); font-style: normal; font-weight: 500; }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 12px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
  }
  .item.active { background: rgba(255, 255, 255, 0.06); }
  .item:hover  { background: rgba(255, 255, 255, 0.04); }

  .item-icon { color: var(--text-disabled); flex-shrink: 0; display: flex; }

  .item-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .item-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-path {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-code);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-open {
    color: var(--text-disabled);
    flex-shrink: 0;
    display: flex;
    opacity: 0;
    transition: opacity var(--transition-fast);
  }
  .item.active .item-open,
  .item:hover  .item-open { opacity: 1; color: var(--accent); }

  .hint {
    font-size: 10px;
    color: var(--text-disabled);
  }

  kbd {
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 4px;
    font-family: var(--font-code);
    font-size: 9px;
    color: var(--text-muted);
  }
</style>
