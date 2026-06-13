<script lang="ts">
  /**
   * Floating "file structure" popover (IntelliJ Ctrl+F12 "File Structure"). Lists
   * the active file's symbols — tracks, functions, constants, imports — in a
   * filterable, keyboard-driven picker; Enter jumps the editor to the declaration.
   * The floating chrome + navigation live in the shared {@link FloatingPicker};
   * this component supplies the live name filter, the header, and the per-symbol
   * row. One mount in the NemusShell.
   */
  import { ListTree, Music4, Braces, Variable, Import } from 'lucide-svelte';
  import type { Component } from 'svelte';
  import FloatingPicker from './FloatingPicker.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { structureStore } from '../stores/structure.svelte';
  import type { NemusSymbol, NemusSymbolKind } from './nemus-lang';

  let filterText = $state('');

  // Match on the display label (e.g. `bassline(root)`) or the bare name — a plain
  // substring is enough for a file's handful of symbols (no fuzzy ranking needed).
  const filtered = $derived.by(() => {
    const q = filterText.trim().toLowerCase();
    const items = structureStore.items;
    if (!q) return items;
    return items.filter(
      (s) => s.label.toLowerCase().includes(q) || s.name.toLowerCase().includes(q),
    );
  });

  // Reset the filter each time the picker opens so it never reopens pre-narrowed.
  $effect(() => {
    if (structureStore.open) filterText = '';
  });

  const KIND_ICON: Record<NemusSymbolKind, Component> = {
    track: Music4, fn: Braces, let: Variable, import: Import,
  };
  const KIND_LABEL: Record<NemusSymbolKind, string> = {
    track: 'track', fn: 'fn', let: 'let', import: 'import',
  };

  function jump(s: NemusSymbol) {
    nemusStore.requestGoto(s.offset, s.line);
    structureStore.close();
  }
</script>

<FloatingPicker
  open={structureStore.open}
  items={filtered}
  width={440}
  maxHeight={400}
  filterable
  bind:filterText
  placeholder="Filter by name…"
  ariaLabel="File structure"
  onSelect={(s) => jump(s)}
  onClose={() => structureStore.close()}
>
  {#snippet header()}
    <ListTree size={12} />
    <span class="st-title">File structure</span>
    <span class="st-count">{filtered.length}</span>
  {/snippet}

  {#snippet row(s: NemusSymbol)}
    {@const Icon = KIND_ICON[s.kind]}
    <span class="st-icon st-{s.kind}"><Icon size={13} /></span>
    <span class="st-label">{s.label}</span>
    <span class="st-kind">{KIND_LABEL[s.kind]}</span>
    <span class="st-line">{s.line}</span>
  {/snippet}

  {#snippet empty()}
    <div class="st-empty">No matching symbol.</div>
  {/snippet}
</FloatingPicker>

<style>
  .st-title { flex: 1; min-width: 0; font-size: 11.5px; color: var(--text-secondary); }
  .st-count {
    font-size: 10px; font-weight: 700; font-variant-numeric: tabular-nums;
    padding: 0 5px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted);
  }

  .st-icon { display: flex; flex-shrink: 0; }
  /* Kind-tinted icons — same hierarchy the editor uses (structure vs content). */
  .st-icon.st-track  { color: var(--grv-syntax-sound, #56b6c2); }
  .st-icon.st-fn     { color: var(--syntax-function, #ffc66d); }
  .st-icon.st-let    { color: var(--syntax-type, #4d78cc); }
  .st-icon.st-import { color: var(--text-muted); }

  .st-label {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: 12px;
    color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .st-kind {
    flex-shrink: 0; font-size: 9px; font-weight: 700; text-transform: uppercase;
    letter-spacing: 0.4px; color: var(--text-muted);
  }
  .st-line {
    flex-shrink: 0; min-width: 28px; text-align: right;
    font-family: var(--font-code); font-size: 10.5px; color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
  }

  .st-empty { padding: 12px 14px; font-size: 11.5px; color: var(--text-muted); }
</style>
