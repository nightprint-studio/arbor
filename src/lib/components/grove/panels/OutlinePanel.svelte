<script lang="ts">
  /** Outline panel — symbols of the active file (`track` / `fn` / `let` /
   *  `import`), grouped by kind. Derived from the SAME Tree-sitter grammar the
   *  editor uses (`outlineFromSource`, gate 3 — client-side, no backend). Click
   *  a symbol to jump the editor to its declaration (via the GroveShell relay).
   *  Cross-file go-to-declaration lives in the editor (Ctrl+Click on a name). */
  import { ListTree, Disc3, SquareFunction, Variable, Import } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { outlineFromSource, type GroveSymbol } from '../editor/grove-lang';

  let entries = $state<GroveSymbol[]>([]);

  // Re-derive the outline from the active source, debounced so a burst of edits
  // coalesces into one parse (the parser is shared + cheap, but no need to spin).
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const src = projectStore.activeSource;
    const path = projectStore.activeFilePath;
    if (timer) clearTimeout(timer);
    if (!path) { entries = []; return; }
    timer = setTimeout(() => {
      void outlineFromSource(src).then((syms) => { entries = syms; });
    }, 200);
    return () => { if (timer) clearTimeout(timer); };
  });

  const GROUPS = [
    { kind: 'track',  label: 'Tracks',    icon: Disc3,          color: 'var(--color-stash, #82aaff)' },
    { kind: 'fn',     label: 'Functions', icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
    { kind: 'let',    label: 'Constants', icon: Variable,       color: 'var(--success)' },
    { kind: 'import', label: 'Imports',   icon: Import,         color: 'var(--info)' },
  ] as const;

  let open = $state<Record<string, boolean>>({});
  const isOpen = (k: string) => open[k] ?? true;
  const ofKind = (k: string) => entries.filter(e => e.kind === k);
</script>

<PanelShell title="Outline" count={entries.length}>
  {#snippet icon()}<ListTree size={13} />{/snippet}

  {#if !projectStore.activeFilePath}
    <EmptyState message="Open a .grove file to see its tracks, functions and constants." />
  {:else if entries.length === 0}
    <EmptyState message="No symbols — this file declares no tracks, functions or constants yet." />
  {:else}
    <div class="outline">
      {#each GROUPS as g (g.kind)}
        {@const items = ofKind(g.kind)}
        {#if items.length > 0}
          {@const Gi = g.icon}
          <SidebarSection
            label={g.label}
            expanded={isOpen(g.kind)}
            onToggle={() => open = { ...open, [g.kind]: !isOpen(g.kind) }}
            badge={items.length}
            iconColor={g.color}
          >
            {#snippet icon()}<Gi size={13} />{/snippet}
            {#each items as e (e.id)}
              <SidebarItem onclick={() => groveStore.requestGoto(e.offset, e.line)}>
                {#snippet icon()}<span style="color: {g.color}; display:flex"><Gi size={12} /></span>{/snippet}
                {e.label}
                {#snippet badges()}<span class="ol-line">:{e.line}</span>{/snippet}
              </SidebarItem>
            {/each}
          </SidebarSection>
        {/if}
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .outline { padding: 4px 0; }
  .ol-line { font-size: 10px; color: var(--text-disabled); font-family: var(--font-code); }
</style>
