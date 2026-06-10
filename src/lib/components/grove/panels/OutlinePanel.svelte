<script lang="ts">
  /** Outline panel — symbols of the active file grouped by kind. */
  import { ListTree, Disc3, SquareFunction, Variable, Import } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { MOCK_PROJECT, MOCK_OUTLINE } from '../mock/data';
  import type { OutlineEntry } from '../mock/types';

  const isSong = $derived(groveStore.activeFileId === MOCK_PROJECT.files[0].id);
  const entries = $derived<OutlineEntry[]>(isSong ? MOCK_OUTLINE : []);

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

  {#if entries.length === 0}
    <EmptyState message="Outline shown for song.grove (mock). Open it to see its symbols." />
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
              <SidebarItem onclick={() => { /* mock: jump to e.line */ }}>
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
