<script lang="ts">
  /**
   * BennuSymbolList — the shared body of the Structure (left) and Outline (right)
   * panels: the active file's Java symbols (class / interface / method / field),
   * grouped by kind, filterable, with jump-to-line on click. Derived from the
   * cheap regex outline (`javaOutline`); replace that seam when a real symbol
   * index lands. One component so both panels stay in sync (single fix surface).
   */
  import { Box, SquareFunction, Variable, Braces, ArrowDownAZ, MoreVertical } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { ArrowRight, Copy } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { javaOutline, type JavaSymbol } from './java-outline';
  import { detectAccessors, flagsFor } from './java-accessors';

  let { title = 'Structure' }: { title?: string } = $props();

  let filter = $state('');
  // Sort mode: by source position (declaration order) or alphabetically by name.
  let sortByName = $state(false);

  const activeSource = $derived(projectStore.activeSource);
  const activePath   = $derived(projectStore.activeFilePath);
  const isJava       = $derived(!!activePath && /\.java$/i.test(activePath));

  // Re-derive on source change, debounced so a burst of edits coalesces.
  let symbols = $state<JavaSymbol[]>([]);
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const src = activeSource;
    const path = activePath;
    if (timer) clearTimeout(timer);
    if (!path || !isJava) { symbols = []; return; }
    timer = setTimeout(() => { symbols = javaOutline(src); }, 180);
    return () => { if (timer) clearTimeout(timer); };
  });

  // Which fields already have a getter / setter / with — same G/S/W markers the
  // Generate modal shows, so Structure and Generate agree. Detected from the same
  // source the symbols came from (pure helper in `java-accessors.ts`).
  const accessorMap = $derived(
    detectAccessors(activeSource, symbols.filter((s) => s.kind === 'field').map((s) => s.name)),
  );

  const q = $derived(filter.trim().toLowerCase());
  const searched = $derived(q ? symbols.filter((s) => s.name.toLowerCase().includes(q)) : symbols);
  const filtered = $derived(
    sortByName ? [...searched].sort((a, b) => a.name.localeCompare(b.name)) : searched,
  );

  const optionsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'sort-pos',  label: 'Sort by position', icon: Braces,     active: !sortByName, onclick: () => (sortByName = false) },
    { kind: 'item', id: 'sort-name', label: 'Sort by name',     icon: ArrowDownAZ, active: sortByName,  onclick: () => (sortByName = true) },
  ]);

  const GROUPS = [
    { kind: 'type',   label: 'Types',   icon: Box,            color: 'var(--info)' },
    { kind: 'method', label: 'Methods', icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
    { kind: 'field',  label: 'Fields',  icon: Variable,       color: 'var(--success)' },
  ] as const;

  // "type" bucket folds class / interface / enum.
  function inGroup(s: JavaSymbol, kind: string): boolean {
    if (kind === 'type') return s.kind === 'class' || s.kind === 'interface' || s.kind === 'enum';
    return s.kind === kind;
  }

  let open = $state<Record<string, boolean>>({});
  const isOpen = (k: string) => open[k] ?? true;

  function copyText(text: string) {
    // Best-effort — clipboard can be denied (permission / focus); swallow.
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(s: JavaSymbol, e: MouseEvent) {
    e.preventDefault();
    // `fqcn` isn't part of the regex outline today, but surface a "Copy FQCN"
    // entry the moment a symbol carries one (real symbol index) — no fork needed.
    const fqcn = (s as JavaSymbol & { fqcn?: string }).fqcn;
    const items: MenuItem[] = [
      { id: 'goto', label: 'Go to', icon: ArrowRight },
      { id: 'copy-name', label: 'Copy name', icon: Copy },
      ...(fqcn ? [{ id: 'copy-fqcn', label: 'Copy FQCN', icon: Copy } as MenuItem] : []),
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'goto':      bennuUiStore.requestGoto(s.line); break;
        case 'copy-name': copyText(s.name); break;
        case 'copy-fqcn': if (fqcn) copyText(fqcn); break;
      }
    });
  }
</script>

<PanelShell {title} count={filtered.length}>
  {#snippet icon()}<Braces size={13} />{/snippet}
  {#snippet actions()}
    <button
      class="ps-btn"
      class:ps-btn-active={sortByName}
      type="button"
      onclick={() => (sortByName = !sortByName)}
      use:tooltip={sortByName ? 'Sorting by name' : 'Sort by name'}
      aria-label="Sort by name"
      aria-pressed={sortByName}
    >
      <ArrowDownAZ size={14} />
    </button>
    <Dropdown items={optionsMenu} position="fixed" direction="down" width="190px">
      {#snippet trigger({ open, toggle })}
        <button class="ps-btn" class:ps-btn-active={open} type="button" onclick={toggle} use:tooltip={'Options'} aria-label="Structure options" aria-haspopup="menu" aria-expanded={open}>
          <MoreVertical size={14} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}
  {#snippet toolbar()}
    <div class="sl-filter">
      <SearchBar bind:query={filter} placeholder="Filter symbols…" showRegex={false} showCounter={false} />
    </div>
  {/snippet}

  {#if !activePath}
    <EmptyState message="Open a file to see its structure." />
  {:else if !isJava}
    <EmptyState message="Structure is available for Java files." />
  {:else if symbols.length === 0}
    <EmptyState message="No symbols found in this file." />
  {:else if filtered.length === 0}
    <EmptyState message={`No symbols match “${filter}”.`} />
  {:else}
    <div class="sl">
      {#each GROUPS as g (g.kind)}
        {@const items = filtered.filter((s) => inGroup(s, g.kind))}
        {#if items.length > 0}
          {@const Gi = g.icon}
          <SidebarSection
            label={g.label}
            expanded={isOpen(g.kind)}
            onToggle={() => (open = { ...open, [g.kind]: !isOpen(g.kind) })}
            badge={items.length}
            iconColor={g.color}
          >
            {#snippet icon()}<Gi size={13} />{/snippet}
            {#each items as s (s.kind + ':' + s.name + ':' + s.line)}
              <SidebarItem
                onclick={() => bennuUiStore.requestGoto(s.line)}
                oncontextmenu={(e) => onRowContextMenu(s, e)}
              >
                {#snippet icon()}<span style="color: {g.color}; display:flex"><Gi size={12} /></span>{/snippet}
                {s.name}
                {#snippet badges()}
                  {#if s.kind === 'field'}
                    {@const acc = flagsFor(accessorMap, s.name)}
                    <span class="sl-acc" aria-hidden="true">
                      <span class="sl-acc-chip" class:on={acc.getter} use:tooltip={acc.getter ? 'Getter exists' : 'No getter'}>G</span>
                      <span class="sl-acc-chip" class:on={acc.setter} use:tooltip={acc.setter ? 'Setter exists' : 'No setter'}>S</span>
                      <span class="sl-acc-chip" class:on={acc.wither} use:tooltip={acc.wither ? 'With-method exists' : 'No with-method'}>W</span>
                    </span>
                  {/if}
                  {#if s.detail}<span class="sl-detail">{s.detail}</span>{/if}
                  <span class="sl-line">:{s.line}</span>
                {/snippet}
              </SidebarItem>
            {/each}
          </SidebarSection>
        {/if}
      {/each}
    </div>
  {/if}
</PanelShell>

<style>
  .sl { padding: 4px 0; }
  .sl-filter { padding: 6px 8px; }
  .sl-detail {
    font-size: 10px; color: var(--text-disabled);
    max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sl-line { font-size: 10px; color: var(--text-disabled); font-family: var(--font-code); }

  /* G / S / W accessor-presence markers on field rows. */
  .sl-acc { display: inline-flex; align-items: center; gap: 2px; }
  .sl-acc-chip {
    display: inline-flex; align-items: center; justify-content: center;
    width: 12px; height: 12px; border-radius: 3px;
    font-size: 8px; font-weight: 700; font-family: var(--font-code);
    color: var(--text-disabled);
    background: var(--bg-overlay);
  }
  .sl-acc-chip.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
</style>
