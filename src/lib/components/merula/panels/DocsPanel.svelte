<script lang="ts">
  /**
   * Docs — the navigable merula language reference, rendered from the canonical
   * DSL catalogue (`referenceStore`, fed by `merula_lang_reference`). Entries are
   * grouped by kind into collapsible sections with a search; each row shows the
   * name, signature, summary, and (on expand) its parameters + example. The
   * authored-once Rust catalogue is the single source — no hardcoded language
   * data here, so it never drifts from the evaluator.
   */
  import { Search } from 'lucide-svelte';
  import {
    Braces, Hash, WandSparkles, Music, Waves, Dice5, Brackets, FileCode2, Terminal,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { referenceStore } from '../stores/reference.svelte';
  import type { MerulaDslEntry, MerulaDslKind } from '$lib/ipc/merula/merula';
  import DocsEntryRow from './DocsEntryRow.svelte';

  // Idempotent: MerulaShell loads this on mount, but a standalone mount still works.
  void referenceStore.load();

  let query = $state('');

  // Display order + label / icon / accent per kind. Drives the section grouping.
  const GROUPS: { kind: MerulaDslKind; label: string; icon: typeof Braces; color: string }[] = [
    { kind: 'keyword',       label: 'Host language',     icon: Braces,        color: 'var(--accent)' },
    { kind: 'island',        label: 'Islands (s / n)',   icon: Music,         color: 'var(--accent)' },
    { kind: 'mini',          label: 'Mini-notation',     icon: Hash,          color: 'var(--info)' },
    { kind: 'note',          label: 'Notes & chords',    icon: Music,         color: 'var(--grv-syntax-note, #e5c07b)' },
    { kind: 'combinator',    label: 'Combinators',       icon: Brackets,      color: 'var(--syntax-function, #ffc66d)' },
    { kind: 'transform',     label: 'Transforms',        icon: WandSparkles,  color: 'var(--color-tag, #c792ea)' },
    { kind: 'generator',     label: 'Generators',        icon: Dice5,         color: 'var(--warning)' },
    { kind: 'signal',        label: 'Signals',           icon: Waves,         color: 'var(--grv-syntax-sound, #56b6c2)' },
    { kind: 'signal_method', label: 'Signal methods',    icon: Waves,         color: 'var(--grv-syntax-sound, #56b6c2)' },
    { kind: 'seq_method',    label: 'Range / list',      icon: FileCode2,     color: 'var(--syntax-function, #ffc66d)' },
    { kind: 'log',           label: 'Logging',           icon: Terminal,      color: 'var(--text-secondary)' },
  ];

  function matches(e: MerulaDslEntry, q: string): boolean {
    return e.name.toLowerCase().includes(q)
      || e.signature.toLowerCase().includes(q)
      || e.summary.toLowerCase().includes(q);
  }

  const sections = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const byKind = new Map<MerulaDslKind, MerulaDslEntry[]>();
    for (const e of referenceStore.entries) {
      if (q && !matches(e, q)) continue;
      const arr = byKind.get(e.kind) ?? [];
      arr.push(e);
      byKind.set(e.kind, arr);
    }
    return GROUPS
      .map((g) => ({ ...g, rows: byKind.get(g.kind) ?? [] }))
      .filter((g) => g.rows.length > 0);
  });

  // Section open state (default: open while searching, else collapsed sections
  // the user hasn't touched stay open too — a reference is most useful expanded).
  let open = $state<Record<string, boolean>>({});
  const isOpen = (id: string) => open[id] ?? true;
  // Track which rows are expanded (id = entry name + kind, unique).
  let expanded = $state<Record<string, boolean>>({});
  const rowId = (e: MerulaDslEntry) => `${e.kind}:${e.name}`;
</script>

<PanelShell title="Language reference">
  {#snippet icon()}<Braces size={13} />{/snippet}
  {#snippet toolbar()}
    <div class="docs-search">
      <Input bind:value={query} placeholder="Search the language…" size="sm">
        {#snippet iconStart()}<Search size={13} />{/snippet}
      </Input>
    </div>
  {/snippet}

  <div class="docs">
    {#if !referenceStore.loaded && referenceStore.entries.length === 0}
      <div class="docs-empty">Loading the language reference…</div>
    {/if}

    {#each sections as sec (sec.kind)}
      {@const Si = sec.icon}
      <SidebarSection
        label={sec.label}
        expanded={isOpen(sec.kind)}
        onToggle={() => open = { ...open, [sec.kind]: !isOpen(sec.kind) }}
        badge={sec.rows.length}
        iconColor={sec.color}
      >
        {#snippet icon()}<Si size={13} />{/snippet}
        {#each sec.rows as e (rowId(e))}
          <DocsEntryRow
            entry={e}
            color={sec.color}
            expanded={!!expanded[rowId(e)]}
            onToggle={() => expanded = { ...expanded, [rowId(e)]: !expanded[rowId(e)] }}
          />
        {/each}
      </SidebarSection>
    {/each}

    {#if referenceStore.loaded && sections.length === 0}
      <div class="docs-empty">No matches for “{query}”.</div>
    {/if}
  </div>
</PanelShell>

<style>
  .docs-search { padding: 6px 8px; }
  .docs { padding: 4px 0 12px; }
  .docs-empty { padding: 14px 12px; font-size: var(--font-size-xs); color: var(--text-muted); font-style: italic; }
</style>
