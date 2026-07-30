<script lang="ts">
  /**
   * BennuSymbolList — the shared body of the Structure (left) and Outline panels:
   * the active file's symbol tree, filterable, with jump-to-line on click. One
   * component so both panels stay in sync (single fix surface).
   *
   * Two outline flavours, picked by the active file's extension:
   *   • JAVA (.java) → a HIERARCHY from `javaStructure`: each type is a root with
   *     `Fields` / `Methods` group buckets and its nested types (recursive). Members
   *     carry a visibility dot (public/protected/private/package) and fields keep the
   *     G/S/W accessor markers so Structure and Generate agree.
   *   • MARKUP (.jsp/.jspf/.tag/.xml/.xsd/.wsdl/.tld/.pom) → a nested element tree
   *     from `markupOutline`: tag name + a key attribute (id/name/var/…), jump-to-line.
   *
   * Rendered with the shared `Tree` widget (controlled expansion, virtualised) —
   * see `BennuSidebar.svelte` for the same pattern on the project tree.
   *
   * SEAM — both outlines are cheap regex scans (`java-outline.ts` /
   * `markup-outline.ts`); replace them when a real symbol index lands.
   *
   * SUPER / inherited members — a collapsed "Inherited" bucket under each type is a
   * planned follow-up. It needs the BE to resolve a superclass's members (the regex
   * scan only sees this file), so it's left as an explicit seam: see
   * `INHERITED_SEAM` below.
   */
  import {
    Box, SquareFunction, Variable, Braces, ArrowDownAZ, MoreVertical,
    FileCode2, Code2, ArrowRight, Copy, ChevronsDownUp, ChevronsUpDown, ArrowUp,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { javaStructure, type JavaNode, type JavaVisibility } from './java-outline';
  import { markupOutline, type MarkupNode } from './markup-outline';
  import { detectAccessors, flagsFor } from './java-accessors';

  let { title = 'Structure' }: { title?: string } = $props();

  let filter = $state('');
  // Sort mode: by source position (declaration order) or alphabetically by name.
  let sortByName = $state(false);

  const activeSource = $derived(projectStore.activeSource);
  const activePath   = $derived(projectStore.activeFilePath);

  const MARKUP_EXT = /\.(jsp|jspf|tag|xml|xsd|wsdl|tld|pom)$/i;
  const isJava   = $derived(!!activePath && /\.java$/i.test(activePath));
  const isMarkup = $derived(!!activePath && MARKUP_EXT.test(activePath));
  const supported = $derived(isJava || isMarkup);

  // ── Node model ──────────────────────────────────────────────────────────────
  // A single tree-node shape the shared `Tree` renders. `java` nodes come from
  // `javaStructure`, `markup` from `markupOutline`; both expose id/name/line so the
  // Tree's getId/getChildren + the row snippet stay uniform.
  type Row =
    | (JavaNode & { flavour: 'java' })
    | (MarkupNode & { flavour: 'markup'; kind: 'element' });

  function tagJava(nodes: JavaNode[]): (JavaNode & { flavour: 'java' })[] {
    return nodes.map((n) => ({
      ...n,
      flavour: 'java' as const,
      children: n.children ? tagJava(n.children) : undefined,
    }));
  }
  function tagMarkup(nodes: MarkupNode[]): (MarkupNode & { flavour: 'markup'; kind: 'element' })[] {
    return nodes.map((n) => ({
      ...n,
      flavour: 'markup' as const,
      kind: 'element' as const,
      children: n.children ? tagMarkup(n.children) : undefined,
    }));
  }

  // Re-derive on source change, debounced so a burst of edits coalesces.
  let roots = $state<Row[]>([]);
  let timer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const src = activeSource;
    const path = activePath;
    const java = isJava;
    const markup = isMarkup;
    if (timer) clearTimeout(timer);
    if (!path || !(java || markup)) { roots = []; return; }
    timer = setTimeout(() => {
      roots = java ? tagJava(javaStructure(src)) : tagMarkup(markupOutline(src));
    }, 180);
    return () => { if (timer) clearTimeout(timer); };
  });

  // Which fields already have a getter / setter / with — same G/S/W markers the
  // Generate modal shows. Detected from the same source, only for Java.
  function collectFieldNames(nodes: Row[], acc: string[]): string[] {
    for (const n of nodes) {
      if (n.flavour === 'java' && n.kind === 'field') acc.push(n.name);
      const kids = (n as { children?: Row[] }).children;
      if (kids) collectFieldNames(kids, acc);
    }
    return acc;
  }
  const accessorMap = $derived(
    isJava ? detectAccessors(activeSource, collectFieldNames(roots, [])) : {},
  );

  // ── Sorting (position vs name) ───────────────────────────────────────────────
  // Sort recursively; group buckets (Fields/Methods) keep their fixed order, only
  // their members are alpha-sorted. Nested types sort by name too.
  function sortNodes(nodes: Row[]): Row[] {
    if (!sortByName) return nodes;
    const sorted = [...nodes].sort((a, b) => a.name.localeCompare(b.name));
    return sorted.map((n) => {
      const kids = (n as { children?: Row[] }).children;
      return kids ? ({ ...n, children: sortNodes(kids) } as Row) : n;
    });
  }
  const displayRoots = $derived(sortNodes(roots));

  // ── Controlled expansion (local, keyed by node id) ───────────────────────────
  // Local to the panel (Structure/Outline don't need cross-window persistence like
  // the project tree). Seed every node open on first render so the tree isn't a wall
  // of collapsed roots; the user's toggles then stick.
  const expanded = new SvelteSet<string>();
  // Guard re-seeding to fire once per file, and only once its roots have actually
  // landed (the `roots` derivation is debounced, so on a fresh file `displayRoots`
  // is briefly empty — seeding then would leave the new file collapsed).
  let seededFor = '';
  $effect(() => {
    const path = activePath;
    const ready = roots.length > 0;
    if (path && ready && path !== seededFor) {
      seededFor = path;
      expanded.clear();
      const walk = (nodes: Row[]) => {
        for (const n of nodes) {
          expanded.add(n.id);
          const kids = (n as { children?: Row[] }).children;
          if (kids) walk(kids);
        }
      };
      walk(displayRoots);
    }
  });
  function onExpandToggle(id: string, next: boolean) {
    if (next) expanded.add(id); else expanded.delete(id);
  }

  // ── Collapse / Expand all (mirrors the project-tree sidebar) ──────────────────
  /** Collapse everything to the top-level roots (clear the expansion set). */
  function collapseAll() { expanded.clear(); }
  /** Expand every node in the tree (walk the full node set into the expansion set). */
  function expandAll() {
    const walk = (nodes: Row[]) => {
      for (const n of nodes) {
        expanded.add(n.id);
        const kids = (n as { children?: Row[] }).children;
        if (kids) walk(kids);
      }
    };
    walk(displayRoots);
  }

  const q = $derived(filter.trim().toLowerCase());

  // ── Options / toolbar ─────────────────────────────────────────────────────────
  const optionsMenu = $derived<DropdownItem[]>([
    { kind: 'item', id: 'sort-pos',  label: 'Sort by position', icon: Braces,     active: !sortByName, onclick: () => (sortByName = false) },
    { kind: 'item', id: 'sort-name', label: 'Sort by name',     icon: ArrowDownAZ, active: sortByName,  onclick: () => (sortByName = true) },
  ]);

  // ── Visuals per node kind ─────────────────────────────────────────────────────
  const KIND_ICON = {
    class:     { icon: Box,            color: 'var(--info)' },
    interface: { icon: Box,            color: 'var(--info)' },
    enum:      { icon: Box,            color: 'var(--info)' },
    record:    { icon: Box,            color: 'var(--info)' },
    method:    { icon: SquareFunction, color: 'var(--color-tag, #c792ea)' },
    field:     { icon: Variable,       color: 'var(--success)' },
    group:     { icon: Braces,         color: 'var(--text-muted)' },
    element:   { icon: Code2,          color: 'var(--color-tag, #c792ea)' },
  } as const;

  function iconFor(node: Row) {
    return KIND_ICON[node.kind as keyof typeof KIND_ICON] ?? { icon: FileCode2, color: 'var(--text-muted)' };
  }

  // Visibility dot: IntelliJ-ish colour coding. Package (default) is muted.
  const VIS_META: Record<JavaVisibility, { color: string; label: string }> = {
    public:    { color: 'var(--success)', label: 'public' },
    protected: { color: 'var(--warning)', label: 'protected' },
    private:   { color: 'var(--error)',   label: 'private' },
    package:   { color: 'var(--text-disabled)', label: 'package-private' },
  };

  // ── Jump-to-line ──────────────────────────────────────────────────────────────
  // Group buckets don't jump (their line is just the owner's); real members do.
  function jumpable(node: Row): boolean {
    return !(node.flavour === 'java' && node.kind === 'group');
  }
  function onRowSelect(node: Row) {
    if (jumpable(node)) bennuUiStore.requestGoto(node.line);
  }

  function copyText(text: string) {
    // Best-effort — clipboard can be denied (permission / focus); swallow.
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(node: Row, e: MouseEvent) {
    if (node.flavour === 'java' && node.kind === 'group') return; // no menu on buckets
    // `fqcn` isn't part of the regex outline today, but surface a "Copy FQCN" entry
    // the moment a node carries one (real symbol index) — no fork needed.
    const fqcn = (node as { fqcn?: string }).fqcn;
    const items: MenuItem[] = [
      { id: 'goto', label: 'Go to', icon: ArrowRight },
      { id: 'copy-name', label: 'Copy name', icon: Copy },
      ...(fqcn ? [{ id: 'copy-fqcn', label: 'Copy FQCN', icon: Copy } as MenuItem] : []),
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'goto':      bennuUiStore.requestGoto(node.line); break;
        case 'copy-name': copyText(node.name); break;
        case 'copy-fqcn': if (fqcn) copyText(fqcn); break;
      }
    });
  }

  // ── INHERITED_SEAM ────────────────────────────────────────────────────────────
  // A collapsed "Inherited" bucket per type (superclass + interface members) is a
  // planned follow-up. The regex scan only sees THIS file, so resolving a
  // superclass's members needs the backend. When the BE endpoint lands:
  //   1. Add a lazy child bucket `{ kind:'group', name:'Inherited', children: null }`
  //      to each Java type root (null children ⇒ Tree shows the chevron via a
  //      `hasChildren` override).
  //   2. On expand, call the BE and splice the resolved members in.
  // The needed contract is reported in the task summary. Nothing is built here.

  const count = $derived(countRows(displayRoots));
  function countRows(nodes: Row[]): number {
    let n = 0;
    for (const r of nodes) {
      if (!(r.flavour === 'java' && r.kind === 'group')) n++;
      const kids = (r as { children?: Row[] }).children;
      if (kids) n += countRows(kids);
    }
    return n;
  }
</script>

<PanelShell {title} {count}>
  {#snippet icon()}<Braces size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" type="button" onclick={collapseAll} use:tooltip={'Collapse all'} aria-label="Collapse all">
      <ChevronsDownUp size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={expandAll} use:tooltip={'Expand all'} aria-label="Expand all">
      <ChevronsUpDown size={14} />
    </button>
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
  {:else if !supported}
    <EmptyState message="Structure is available for Java and XML/JSP files." />
  {:else if roots.length === 0}
    <EmptyState message="No symbols found in this file." />
  {:else}
    <div class="sl">
      <Tree
        nodes={displayRoots}
        getId={(n) => n.id}
        getChildren={(n) => (n as { children?: Row[] }).children}
        expandedIds={expanded}
        {onExpandToggle}
        {filter}
        selectable={jumpable}
        ariaLabel="{title} symbols"
        onSelect={onRowSelect}
        onContextMenu={onRowContextMenu}
        rowClass={(ctx) => (ctx.node.flavour === 'java' && ctx.node.kind === 'group' ? 'tree-row-section' : '')}
        emptyState={filterEmpty}
      >
        {#snippet row(ctx: RowSnippetCtx<Row>)}
          {@const node = ctx.node}
          {@const meta = iconFor(node)}
          {@const Icon = meta.icon}
          {#if node.flavour === 'java' && node.visibility && node.kind !== 'group'}
            <span
              class="sl-vis"
              style="background: {VIS_META[node.visibility].color}"
              use:tooltip={VIS_META[node.visibility].label}
              aria-hidden="true"
            ></span>
          {:else}
            <span class="sl-vis sl-vis-none" aria-hidden="true"></span>
          {/if}
          <span class="tree-icon" style="color: {meta.color}"><Icon size={13} /></span>
          <span class="tree-label">{node.name}</span>
          {#if node.flavour === 'java' && node.kind === 'method' && node.overrides}
            <span class="sl-override" use:tooltip={'Overrides / implements a supertype member'} aria-label="overrides a supertype member">
              <ArrowUp size={11} />
            </span>
          {/if}
          {#if node.flavour === 'java' && node.kind === 'field'}
            {@const acc = flagsFor(accessorMap, node.name)}
            <span class="sl-acc" aria-hidden="true">
              <span class="sl-acc-chip" class:on={acc.getter} use:tooltip={acc.getter ? 'Getter exists' : 'No getter'}>G</span>
              <span class="sl-acc-chip" class:on={acc.setter} use:tooltip={acc.setter ? 'Setter exists' : 'No setter'}>S</span>
              <span class="sl-acc-chip" class:on={acc.wither} use:tooltip={acc.wither ? 'With-method exists' : 'No with-method'}>W</span>
            </span>
          {/if}
          {#if node.detail}<span class="sl-detail">{node.detail}</span>{/if}
          {#if !(node.flavour === 'java' && node.kind === 'group')}
            <span class="sl-line">:{node.line}</span>
          {/if}
        {/snippet}
      </Tree>
    </div>
  {/if}
</PanelShell>

{#snippet filterEmpty()}
  <EmptyState message={`No symbols match “${filter}”.`} />
{/snippet}

<style>
  .sl { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 0; }
  .sl-filter { padding: 6px 8px; }
  .sl-detail {
    font-size: var(--font-size-2xs); color: var(--text-disabled);
    max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sl-line { font-size: var(--font-size-2xs); color: var(--text-disabled); font-family: var(--font-code); flex-shrink: 0; }

  /* Visibility dot — a fixed-width slot so labels line up whether or not a node
     carries visibility (groups / markup elements render an empty slot). */
  .sl-vis {
    width: 7px; height: 7px; border-radius: 50%;
    flex-shrink: 0;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--bg-base) 60%, transparent);
  }
  .sl-vis-none { background: transparent; box-shadow: none; }

  /* @Override marker on method rows — a small accent arrow, IntelliJ's "overrides" cue. */
  .sl-override {
    display: inline-flex; align-items: center; justify-content: center;
    flex-shrink: 0; color: var(--info);
    opacity: 0.85;
  }

  /* G / S / W accessor-presence markers on field rows. */
  .sl-acc { display: inline-flex; align-items: center; gap: 2px; flex-shrink: 0; }
  .sl-acc-chip {
    display: inline-flex; align-items: center; justify-content: center;
    width: 12px; height: 12px; border-radius: 3px;
    font-size: var(--font-size-3xs); font-weight: 700; font-family: var(--font-code);
    color: var(--text-disabled);
    background: var(--bg-overlay);
  }
  .sl-acc-chip.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
</style>
