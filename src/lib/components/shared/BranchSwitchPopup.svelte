<script lang="ts">
  /**
   * BranchSwitchPopup — a compact, filterable, keyboard-first branch switcher
   * anchored at a screen point (footer branch chip, context menu, …). Type to
   * filter, ↑/↓ to move, Enter to switch, Esc to close. The current branch is
   * marked and not selectable. The host performs the actual checkout and owns
   * the `busy` flag while it runs.
   */
  import { tick } from 'svelte';
  import { Check, GitBranch, Search } from 'lucide-svelte';
  import Spinner from './ui/Spinner.svelte';
  import type { FsBranch } from '$lib/ipc/fs';

  let {
    x = 0,
    y = 0,
    branches,
    busy = false,
    onSelect,
    onClose,
  }: {
    x?: number;
    y?: number;
    branches: FsBranch[];
    busy?: boolean;
    onSelect: (name: string) => void;
    onClose: () => void;
  } = $props();

  let query = $state('');
  let highlighted = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);
  let panelEl = $state<HTMLElement | null>(null);
  // svelte-ignore state_referenced_locally
  let px = $state(x);
  // svelte-ignore state_referenced_locally
  let py = $state(y);

  const filtered = $derived(
    query.trim()
      ? branches.filter(b => b.name.toLowerCase().includes(query.trim().toLowerCase()))
      : branches,
  );

  // Keep the highlight in range as the filter narrows; prefer the current
  // branch on first open so ↓ lands on a switchable neighbour.
  $effect(() => {
    if (highlighted >= filtered.length) highlighted = Math.max(0, filtered.length - 1);
  });

  $effect(() => {
    inputEl?.focus();
  });

  // Clamp into the viewport once measured, opening upward from a low anchor.
  $effect(() => {
    if (!panelEl) return;
    const r = panelEl.getBoundingClientRect();
    px = Math.max(8, Math.min(x, window.innerWidth - r.width - 8));
    py = Math.max(8, Math.min(y, window.innerHeight - r.height - 8));
  });

  function choose(b: FsBranch) {
    if (busy || b.is_head) return;
    onSelect(b.name);
  }

  async function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onClose(); return; }
    if (e.key === 'ArrowDown') { e.preventDefault(); highlighted = Math.min(highlighted + 1, filtered.length - 1); scrollHl(); return; }
    if (e.key === 'ArrowUp')   { e.preventDefault(); highlighted = Math.max(highlighted - 1, 0); scrollHl(); return; }
    if (e.key === 'Enter')     { e.preventDefault(); const b = filtered[highlighted]; if (b) choose(b); return; }
  }

  async function scrollHl() {
    await tick();
    panelEl?.querySelector<HTMLElement>('.bsp-row.hl')?.scrollIntoView({ block: 'nearest' });
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- Outside-click catcher (covers Tauri drag regions, like ContextMenu). -->
<div class="bsp-backdrop" role="presentation" onpointerdown={(e) => { if (e.button !== 2) onClose(); }}></div>

<div bind:this={panelEl} class="bsp" style="left: {px}px; top: {py}px" role="dialog" aria-label="Switch branch" tabindex="-1">
  <div class="bsp-search">
    <Search size={13} class="bsp-search-ico" />
    <input
      bind:this={inputEl}
      bind:value={query}
      class="bsp-input"
      type="text"
      placeholder="Switch branch…"
      autocomplete="off"
      spellcheck="false"
      disabled={busy}
      oninput={() => highlighted = 0}
    />
    {#if busy}<Spinner size="sm" />{/if}
  </div>
  <div class="bsp-list" role="listbox" aria-label="Branches">
    {#each filtered as b, i (b.name)}
      <button
        class="bsp-row"
        class:hl={i === highlighted}
        class:current={b.is_head}
        role="option"
        aria-selected={i === highlighted}
        disabled={busy || b.is_head}
        onmousemove={() => highlighted = i}
        onclick={() => choose(b)}
      >
        <span class="bsp-ico">{#if b.is_head}<Check size={13} />{:else}<GitBranch size={13} />{/if}</span>
        <span class="bsp-name">{b.name}</span>
        {#if b.is_head}<span class="bsp-cur">current</span>{/if}
      </button>
    {:else}
      <div class="bsp-empty">No branches match “{query}”</div>
    {/each}
  </div>
</div>

<style>
  .bsp-backdrop { position: fixed; inset: 0; z-index: calc(var(--z-menu) - 1); background: transparent; }
  .bsp {
    position: fixed; z-index: var(--z-menu);
    width: 280px; max-height: 360px; display: flex; flex-direction: column;
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); box-shadow: var(--shadow-popup);
    overflow: hidden;
  }
  .bsp-search {
    display: flex; align-items: center; gap: 6px; padding: 7px 9px;
    border-bottom: 1px solid var(--border-subtle, var(--border)); flex-shrink: 0;
  }
  .bsp-search :global(.bsp-search-ico) { color: var(--text-muted); flex-shrink: 0; }
  .bsp-input {
    flex: 1; min-width: 0; background: none; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
  }
  .bsp-input::placeholder { color: var(--text-muted); }
  .bsp-list { overflow-y: auto; padding: 4px; scrollbar-width: thin; scrollbar-color: var(--scrollbar-thumb) transparent; }
  .bsp-row {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 5px 8px; border: none; background: none; border-radius: var(--radius-sm);
    cursor: pointer; text-align: left; color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
  }
  .bsp-row.hl:not(:disabled) { background: var(--bg-selected); }
  .bsp-row.current { color: var(--accent); cursor: default; }
  .bsp-row:disabled:not(.current) { opacity: 0.5; cursor: not-allowed; }
  .bsp-ico { display: inline-flex; align-items: center; flex-shrink: 0; color: var(--text-muted); }
  .bsp-row.current .bsp-ico { color: var(--accent); }
  .bsp-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono, monospace); }
  .bsp-cur { font-size: 10px; color: var(--accent); flex-shrink: 0; }
  .bsp-empty { padding: 14px 10px; text-align: center; color: var(--text-muted); font-size: 12px; }
</style>
