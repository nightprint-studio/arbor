<script lang="ts" module>
  /** One binding: the chord parts plus what it does. */
  export interface ShortcutEntry {
    keys: string[];
    description: string;
  }

  /** A titled group of bindings ("Navigation", "Database", …). */
  export interface ShortcutGroup {
    label: string;
    shortcuts: ShortcutEntry[];
  }
</script>

<script lang="ts">
  /**
   * ShortcutsReference — the searchable card grid of keyboard bindings.
   *
   * The read-only reference used by product windows whose shortcuts are
   * window-local (Tyto, Picus, …) rather than rebindable through the global
   * keybindings store. It owns the search, the grouping and the layout; the host
   * supplies the list and the surrounding modal chrome.
   *
   * NOTE (shared/ui contract): the only import from `shared/internal/` is `Kbd`,
   * which is the canonical way to render a chord in Arbor — the same documented
   * exception `Dropdown` already makes for shortcut hints.
   */
  import { Search } from 'lucide-svelte';
  import SearchBar from './SearchBar.svelte';
  import Kbd from '../internal/Kbd.svelte';

  interface Props {
    groups: ShortcutGroup[];
    /** Focus the filter box on mount. */
    autofocus?: boolean;
    placeholder?: string;
  }

  let { groups, autofocus = true, placeholder = 'Filter shortcuts…' }: Props = $props();

  let query = $state('');

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return groups;
    return groups
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

<div class="sr">
  <div class="sr-toolbar">
    <SearchBar
      bind:query
      showRegex={false}
      showCounter={false}
      {placeholder}
      {autofocus}
      ariaLabel="Filter keyboard shortcuts"
    />
  </div>

  <div class="sr-scroll">
    {#if totalShown === 0}
      <div class="sr-empty">
        <Search size={26} strokeWidth={1.5} />
        <p>No shortcut matches “{query.trim()}”.</p>
      </div>
    {:else}
      <div class="sr-grid">
        {#each filtered as group (group.label)}
          <section class="sr-card">
            <div class="sr-card-head">
              <h3 class="sr-card-title">{group.label}</h3>
              <span class="sr-card-count">{group.shortcuts.length}</span>
            </div>
            <ul class="sr-rows">
              {#each group.shortcuts as s (s.description)}
                <li class="sr-row">
                  <span class="sr-desc">{s.description}</span>
                  <span class="sr-keys"><Kbd keys={s.keys} size="sm" /></span>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .sr { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .sr-toolbar {
    flex-shrink: 0;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .sr-scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 16px; }

  .sr-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    align-items: start;
  }

  .sr-card {
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px 12px;
  }
  .sr-card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px 8px;
    margin-bottom: 5px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sr-card-title {
    flex: 1;
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .sr-card-count {
    flex-shrink: 0;
    padding: 1px 7px;
    background: var(--bg-overlay);
    border-radius: 999px;
    font-size: var(--font-size-2xs);
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }

  .sr-rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .sr-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .sr-row:hover { background: var(--bg-hover); }
  .sr-desc { flex: 1; font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.35; }
  .sr-keys { flex-shrink: 0; }

  .sr-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 56px 16px;
    color: var(--text-disabled);
  }
  .sr-empty p { margin: 0; font-size: var(--font-size-sm); }
</style>
