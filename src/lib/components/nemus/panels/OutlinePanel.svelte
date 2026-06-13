<script lang="ts">
  /** Outline panel — symbols of the active file (`track` / `fn` / `let` /
   *  `import`), grouped by kind. Derived from the SAME Tree-sitter grammar the
   *  editor uses (`outlineFromSource`, gate 3 — client-side, no backend). Click
   *  a symbol to jump the editor to its declaration (via the NemusShell relay).
   *  Cross-file go-to-declaration lives in the editor (Ctrl+Click on a name). */
  import { ListTree, Disc3, SquareFunction, Variable, Import, ChevronRight, Layers } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { sectionColor } from '../palette';
  import { projectStore } from '../stores/project.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { outlineTreeFromSource, type NemusOutlineNode } from '../editor/nemus-lang';

  let entries = $state<NemusOutlineNode[]>([]);

  // Re-derive the outline from the active source, debounced so a burst of edits
  // coalesces into one parse (the parser is shared + cheap, but no need to spin).
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const src = projectStore.activeSource;
    const path = projectStore.activeFilePath;
    if (timer) clearTimeout(timer);
    if (!path) { entries = []; return; }
    timer = setTimeout(() => {
      void outlineTreeFromSource(src).then((syms) => { entries = syms; });
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

  // Per-track expansion of the nested section list (collapsed by default so the
  // Tracks group stays scannable on dense arrangements).
  let trackOpen = $state<Record<string, boolean>>({});
  const isTrackOpen = (id: string) => trackOpen[id] ?? false;
  function toggleTrack(id: string) { trackOpen = { ...trackOpen, [id]: !isTrackOpen(id) }; }
</script>

<PanelShell title="Outline" count={entries.length}>
  {#snippet icon()}<ListTree size={13} />{/snippet}

  {#if !projectStore.activeFilePath}
    <EmptyState message="Open a .nemus file to see its tracks, functions and constants." />
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
              {#if g.kind === 'track' && e.children?.length}
                {@const secs = e.children}
                <!-- Expandable track: row jumps to the track, the chevron toggles
                     its named sections (nested, indented). -->
                <SidebarItem onclick={() => nemusStore.requestGoto(e.offset, e.line)}>
                  {#snippet icon()}
                    <button class="ol-chevron" class:open={isTrackOpen(e.id)}
                            onclick={(ev) => { ev.stopPropagation(); toggleTrack(e.id); }}
                            aria-label={isTrackOpen(e.id) ? 'Collapse sections' : 'Expand sections'}
                            aria-expanded={isTrackOpen(e.id)}>
                      <ChevronRight size={12} />
                    </button>
                  {/snippet}
                  {e.label}
                  {#snippet badges()}<span class="ol-secount">{secs.length}</span>{/snippet}
                </SidebarItem>
                {#if isTrackOpen(e.id)}
                  {#each secs as s (s.id)}
                    <SidebarItem indent={26} onclick={() => nemusStore.requestGoto(s.offset, s.line)}>
                      {#snippet icon()}<span class="ol-sec-dot" style="--sc: {sectionColor(s.name)}"><Layers size={11} /></span>{/snippet}
                      {s.label}
                      {#snippet badges()}<span class="ol-line">:{s.line}</span>{/snippet}
                    </SidebarItem>
                  {/each}
                {/if}
              {:else}
                <SidebarItem onclick={() => nemusStore.requestGoto(e.offset, e.line)}>
                  {#snippet icon()}<span style="color: {g.color}; display:flex"><Gi size={12} /></span>{/snippet}
                  {e.label}
                  {#snippet badges()}<span class="ol-line">:{e.line}</span>{/snippet}
                </SidebarItem>
              {/if}
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
  .ol-secount {
    font-size: 9px; font-weight: 700; line-height: 1;
    padding: 2px 5px; border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text-secondary);
  }
  /* Track expand/collapse chevron — replaces the static track icon so the row
     reads as a disclosure without stealing the row's jump-to-track click. */
  .ol-chevron {
    display: flex; align-items: center; justify-content: center;
    width: 14px; height: 14px; padding: 0;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-muted); border-radius: var(--radius-sm);
    transition: transform var(--transition-fast), color var(--transition-fast);
  }
  .ol-chevron:hover { color: var(--text-primary); }
  .ol-chevron.open { transform: rotate(90deg); }
  .ol-sec-dot { display: flex; color: var(--sc); }
</style>
