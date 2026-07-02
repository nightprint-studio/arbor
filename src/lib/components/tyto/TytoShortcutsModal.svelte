<script lang="ts">
  /**
   * TytoShortcutsModal — a searchable reference of Tyto's window shortcuts.
   *
   * Read-only (unlike Corvus's rebindable ShortcutsModal, whose bindings live in
   * the global keybindings store): Tyto's shortcuts are window-local, sourced
   * from the shared TYTO_SHORTCUTS list. Same searchable card layout so it reads
   * like the rest of the suite.
   */
  import { Keyboard, Search } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { TYTO_SHORTCUTS } from './tyto-shortcuts';

  let { onClose }: { onClose: () => void } = $props();

  let query = $state('');

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return TYTO_SHORTCUTS;
    return TYTO_SHORTCUTS
      .map((g) => ({
        label: g.label,
        shortcuts: g.shortcuts.filter(
          (s) =>
            s.description.toLowerCase().includes(q) ||
            s.keys.join('+').toLowerCase().includes(q) ||
            g.label.toLowerCase().includes(q),
        ),
      }))
      .filter((g) => g.shortcuts.length > 0);
  });

  const totalShown = $derived(filtered.reduce((n, g) => n + g.shortcuts.length, 0));
</script>

<Modal {onClose} width="640px" height="560px" padBody={false} ariaLabel="Tyto keyboard shortcuts">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Keyboard size={14} />
      <span class="modal-title">Keyboard Shortcuts</span>
    </ModalHeader>
  {/snippet}

  <div class="sc-body">
    <div class="sc-toolbar">
      <SearchBar
        bind:query
        showRegex={false}
        showCounter={false}
        placeholder="Filter shortcuts…"
        ariaLabel="Filter keyboard shortcuts"
        autofocus
      />
    </div>

    <div class="sc-scroll">
      {#if totalShown === 0}
        <div class="sc-empty">
          <Search size={26} strokeWidth={1.5} />
          <p>No shortcut matches “{query.trim()}”.</p>
        </div>
      {:else}
        <div class="sc-grid">
          {#each filtered as group (group.label)}
            <section class="sc-card">
              <div class="sc-card-head">
                <h3 class="sc-card-title">{group.label}</h3>
                <span class="sc-card-count">{group.shortcuts.length}</span>
              </div>
              <ul class="sc-rows">
                {#each group.shortcuts as s (s.description)}
                  <li class="sc-row">
                    <span class="sc-desc">{s.description}</span>
                    <span class="sc-keys"><Kbd keys={s.keys} size="sm" /></span>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .sc-body { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .sc-toolbar {
    flex-shrink: 0;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .sc-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 16px; }

  .sc-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    align-items: start;
  }

  .sc-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px 12px;
  }
  .sc-card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px 8px;
    margin-bottom: 5px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sc-card-title {
    flex: 1;
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .sc-card-count {
    flex-shrink: 0;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 1px 7px;
  }

  .sc-rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .sc-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .sc-row:hover { background: var(--bg-hover); }
  .sc-desc { flex: 1; font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.35; }
  .sc-keys { flex-shrink: 0; }

  .sc-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 56px 16px;
    color: var(--text-disabled);
  }
  .sc-empty p { margin: 0; font-size: var(--font-size-sm); }
</style>
