<script module lang="ts">
  import type { Component } from 'svelte';
  import type { IconComponent } from '$lib/types/icon';
  import type { TooltipArg } from '$lib/actions/tooltip';

  export interface DocsNavItem { id: string; label: string; icon?: IconComponent; }
  export interface DocsNavGroup { id: string; label: string; icon?: IconComponent; items: DocsNavItem[]; }

  /**
   * A topic whose body is a raw HTML string instead of a topic component —
   * documentation that only exists at runtime and therefore cannot be imported:
   * a plugin's `doc.html`, a manual fetched from a marketplace, a generated
   * reference. Ids must not collide with the keys of `sections`.
   */
  export interface DocsHtmlItem {
    id: string;
    label: string;
    /** Rendered with `{@html}` inside the same typography block topics get. */
    html: string;
    /** Dimmed and struck through in the nav — the entry is still readable. */
    muted?: boolean;
    /** Left out of the exported document (Markdown and HTML alike). */
    excludeFromExport?: boolean;
    /** Small uppercase pill after the label, e.g. `disabled`. */
    pill?: string;
    /** Tooltip for the nav entry — the only place a runtime topic can explain itself. */
    tooltip?: TooltipArg;
    /** Heading this entry gets in the export. Defaults to `label`. */
    exportName?: string;
  }

  /** A nav group whose items arrive at runtime. Rendered after `navGroups`. */
  export interface DocsHtmlGroup {
    id: string;
    label: string;
    icon?: IconComponent;
    items: DocsHtmlItem[];
  }
</script>

<script lang="ts">
  /**
   * DocsShell — the shared full-page documentation panel (the look Arbor's
   * DocsPanel established): a large modal with a grouped, searchable nav on the
   * left and a scrollable content pane on the right. App-agnostic — the host
   * passes a flat `topItems` list, collapsible `navGroups`, and a `sections`
   * map (id → topic component). Each topic component writes plain semantic HTML;
   * the shared `PluginDocBlock` supplies the typography baseline.
   *
   * Topics that only exist at runtime (a plugin's `doc.html`) come in through
   * `htmlGroups` instead: same nav, same search, same export, but the body is an
   * HTML string rather than a component. Nothing else distinguishes them, which
   * is the point — the shell never learns which product it is serving.
   *
   * Search filters the nav (label + full-text via an offscreen index) and
   * highlights matches in the active section (F3 / Shift+F3 to cycle).
   *
   * It also **exports** — Markdown or styled HTML — through the same
   * `docs-export` converter Arbor's own DocsPanel uses. It lives here rather than
   * in each product's panel for the obvious reason: the conversion is identical,
   * and a product whose documentation cannot leave the window is documentation
   * nobody can put in a ticket, a wiki or a repository.
   */
  import { tick, untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ChevronRight, FileDown } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from './ui/Button.svelte';
  import Dropdown, { type DropdownItem } from './ui/Dropdown.svelte';
  import Spinner from './ui/Spinner.svelte';
  import SearchBar from './ui/SearchBar.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import PluginDocBlock from '$lib/components/shared/internal/PluginDocBlock.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { animStore } from '$lib/stores/animations.svelte';
  import { fsWriteTextFile } from '$lib/ipc/fs';
  import { notificationsStore } from '$lib/feedback/stores/notifications.svelte';
  import {
    buildReadme, buildHtmlExport,
    type SectionEntry, type HtmlSectionEntry, type PluginDocEntry,
  } from '$lib/utils/docs-export';
  import {
    compileQuery, highlightLabel, textMatches, injectHighlights, clearHighlights,
  } from '$lib/utils/text-search';

  let {
    topItems = [],
    navGroups = [],
    htmlGroups = [],
    sections,
    onClose,
    title = 'Documentation',
    headerIcon,
    initialSection,
    initialOpenGroup,
    width = '1040px',
    height = '700px',
    product,
    fileBase,
    prebuildSearchIndex = false,
  }: {
    topItems?: DocsNavItem[];
    navGroups?: DocsNavGroup[];
    /** Runtime topics, rendered as `{@html}`. Pass a `$derived` value: the shell
     *  re-indexes and re-renders whenever the list changes underneath it. */
    htmlGroups?: DocsHtmlGroup[];
    sections: Record<string, Component>;
    onClose: () => void;
    title?: string;
    headerIcon?: IconComponent;
    initialSection?: string;
    /** Nav group expanded on open. Defaults to the first one; pass `null` for a
     *  fully collapsed nav, which is what a long topic list wants. */
    initialOpenGroup?: string | null;
    width?: string;
    height?: string;
    /** Product name in the exported heading. Defaults to the panel's title. */
    product?: string;
    /** Base file name the save dialog proposes. Defaults to a slug of `product`. */
    fileBase?: string;
    /** Build the full-text index shortly after opening instead of on the first
     *  query. Worth it for large doc sets, where extraction is long enough to be
     *  felt as a stutter on the first keystroke. */
    prebuildSearchIndex?: boolean;
  } = $props();

  const productName = $derived(
    product ?? (title.replace(/\s*Documentation\s*$/i, '').trim() || 'Arbor'),
  );
  const exportBase = $derived(fileBase ?? `${productName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-docs`);

  /** One row of the nav. Runtime topics carry a little more chrome than static
   *  ones (a pill, a tooltip, a dimmed state) — everything else is identical, so
   *  both kinds of group render through the same markup below. */
  type NavRow = DocsNavItem & { pill?: string; tooltip?: TooltipArg; muted?: boolean };
  interface RenderGroup { id: string; label: string; icon?: IconComponent; items: NavRow[]; }

  const htmlItems = $derived(htmlGroups.flatMap((g) => g.items));
  const renderGroups = $derived<RenderGroup[]>([
    ...navGroups,
    ...htmlGroups.map((g) => ({
      id: g.id,
      label: g.label,
      icon: g.icon,
      items: g.items.map((i) => ({
        id: i.id, label: i.label, pill: i.pill, tooltip: i.tooltip, muted: i.muted,
      })),
    })),
  ]);

  let activeSection = $state(
    untrack(() => initialSection ?? topItems[0]?.id ?? navGroups[0]?.items[0]?.id ?? ''),
  );
  let groupOpen = $state<Record<string, boolean>>(
    untrack(() => {
      const open = initialOpenGroup === undefined ? navGroups[0]?.id : initialOpenGroup;
      return Object.fromEntries(navGroups.map((g) => [g.id, g.id === open]));
    }),
  );

  const orderedSections = $derived([
    ...topItems.map((i) => ({ id: i.id, label: i.label })),
    ...navGroups.flatMap((g) => g.items.map((i) => ({ id: i.id, label: i.label }))),
    ...htmlItems.map((i) => ({ id: i.id, label: i.label })),
  ]);

  const activeHtml = $derived(htmlItems.find((i) => i.id === activeSection));

  function selectSection(id: string) {
    activeSection = id;
    for (const g of renderGroups) {
      if (g.items.some((it) => it.id === id)) { groupOpen[g.id] = true; break; }
    }
  }

  // ── Search ──────────────────────────────────────────────────────────────
  let searchQuery = $state('');
  let searchRegex = $state(false);
  /** The hidden container every section is rendered into. Shared by the search
   *  index and by the export — both need every topic mounted at once, and two
   *  containers would be two copies of the same tree. */
  let offscreenEl = $state<HTMLElement | null>(null);
  let contentEl = $state<HTMLElement | null>(null);
  let extracting = $state(false);
  let matchingIds = $state<Set<string>>(new Set());
  let labelMatchIds = $state<Set<string>>(new Set());
  let contentMarks = $state<HTMLElement[]>([]);
  let currentMarkIdx = $state(0);

  const searchActive = $derived(searchQuery.trim().length > 0);
  const compiledQuery = $derived.by(() => compileQuery(searchQuery.trim(), { regex: searchRegex }));
  const regexInvalid = $derived(searchRegex && searchQuery.trim().length > 0 && compiledQuery === null);

  const textCache = new Map<string, string>();
  function cssEscape(s: string): string {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/(["\\])/g, '\\$1');
  }
  function extractText(root: HTMLElement): string {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        let p = node.parentElement;
        while (p && p !== root) {
          const t = p.tagName;
          if (t === 'PRE' || t === 'CODE' || t === 'SCRIPT' || t === 'STYLE') return NodeFilter.FILTER_REJECT;
          p = p.parentElement;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const parts: string[] = [];
    let n: Node | null;
    while ((n = walker.nextNode())) parts.push(n.nodeValue ?? '');
    return parts.join(' ');
  }
  let _cacheBuilding: Promise<void> | null = null;
  async function ensureCache(): Promise<void> {
    if (textCache.size > 0) return;
    if (_cacheBuilding) return _cacheBuilding;
    _cacheBuilding = (async () => {
      extracting = true;
      await tick(); await tick();
      if (offscreenEl) {
        for (const s of orderedSections) {
          const el = offscreenEl.querySelector<HTMLElement>(`[data-section="${cssEscape(s.id)}"]`);
          if (el) textCache.set(s.id, extractText(el));
        }
      }
      extracting = false;
      _cacheBuilding = null;
    })();
    return _cacheBuilding;
  }

  async function doSearch() {
    if (!searchActive || !compiledQuery) {
      matchingIds = new Set(); labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    await ensureCache();
    const re = compiledQuery;
    const matches = new Set<string>();
    const labels = new Set<string>();
    for (const s of orderedSections) {
      const labelHit = textMatches(s.label, re);
      if (labelHit) labels.add(s.id);
      if (labelHit || textMatches(textCache.get(s.id) ?? '', re)) matches.add(s.id);
    }
    matchingIds = matches; labelMatchIds = labels;
    for (const g of renderGroups) if (g.items.some((i) => matches.has(i.id))) groupOpen[g.id] = true;
    if (!matches.has(activeSection)) {
      const first = orderedSections.find((s) => matches.has(s.id));
      if (first) activeSection = first.id;
    }
    await applyContentHighlights();
  }

  let _searchTimer: ReturnType<typeof setTimeout> | null = null;
  function onSearchInput() {
    if (_searchTimer) clearTimeout(_searchTimer);
    _searchTimer = setTimeout(doSearch, 120);
  }
  $effect(() => { searchRegex; if (searchActive) doSearch(); });
  function clearSearch() {
    searchQuery = ''; matchingIds = new Set(); labelMatchIds = new Set();
    if (contentEl) { clearHighlights(contentEl, 'docs-match'); contentMarks = []; currentMarkIdx = 0; }
  }

  // Runtime topics can appear, vanish or be rewritten while the panel is open
  // (a plugin reload). The signature covers the bodies, not just the count:
  // swapping one entry for another of the same size is exactly the case a
  // length check would miss, and a stale index answers queries with topics that
  // are no longer there. Rebuilding is lazy — the next query pays for it.
  let _htmlSignature = '';
  $effect(() => {
    const sig = htmlItems.map((i) => `${i.id}:${i.html.length}`).join('\u0000');
    if (sig === _htmlSignature) return;
    _htmlSignature = sig;
    untrack(() => {
      if (textCache.size > 0) { textCache.clear(); _cacheBuilding = null; }
      if (searchActive) doSearch();
    });
  });

  // Opt-in warm-up: deferred a beat so the modal paints first.
  let _prebuildStarted = false;
  $effect(() => {
    if (!prebuildSearchIndex || _prebuildStarted) return;
    _prebuildStarted = true;
    const t = setTimeout(() => { void ensureCache(); }, 80);
    return () => clearTimeout(t);
  });

  async function applyContentHighlights() {
    await tick(); await tick();
    if (!contentEl || !contentEl.isConnected) return;
    clearHighlights(contentEl, 'docs-match');
    contentMarks = []; currentMarkIdx = 0;
    if (!compiledQuery || !searchActive) return;
    const marks = injectHighlights(contentEl, compiledQuery, { className: 'docs-match' });
    contentMarks = marks;
    if (marks.length > 0) { marks[0].classList.add('current'); marks[0].scrollIntoView({ block: 'center', behavior: 'auto' }); }
  }
  $effect(() => { activeSection; untrack(() => { applyContentHighlights(); }); });

  function gotoMark(idx: number) {
    if (contentMarks.length === 0) return;
    const wrapped = ((idx % contentMarks.length) + contentMarks.length) % contentMarks.length;
    contentMarks.forEach((m) => m.classList.remove('current'));
    const target = contentMarks[wrapped];
    target.classList.add('current');
    target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    currentMarkIdx = wrapped;
  }
  function jumpSection(dir: 1 | -1) {
    const order = orderedSections.filter((s) => matchingIds.has(s.id)).map((s) => s.id);
    if (!order.length) return;
    const i = order.indexOf(activeSection);
    activeSection = i === -1 ? order[0] : order[(i + dir + order.length) % order.length];
  }
  function nextMatch() { if (contentMarks.length) gotoMark(currentMarkIdx + 1); else jumpSection(+1); }
  function prevMatch() { if (contentMarks.length) gotoMark(currentMarkIdx - 1); else jumpSection(-1); }

  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'F3') { e.preventDefault(); if (e.shiftKey) prevMatch(); else nextMatch(); }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ── Export ──────────────────────────────────────────────────────────────
  //
  // Every topic is mounted offscreen, its rendered DOM is converted, and only
  // then is a destination asked for. Converting first is what lets the refusal
  // — an unwritable path, a cancelled dialog — cost nothing: there is no
  // half-written file to clean up, and the picker is the last step rather than
  // the first.

  let exportMode = $state(false);
  let exporting = $state(false);
  let pendingExport = $state<{ content: string; defaultName: string } | null>(null);

  /** The section list with each group's label carried by its first item, which is
   *  how the HTML export draws its table of contents. */
  const exportOrder = $derived([
    ...topItems.map((i) => ({ id: i.id, label: i.label, groupLabel: undefined as string | undefined })),
    ...navGroups.flatMap((g) =>
      g.items.map((item, idx) => ({
        id: item.id,
        label: item.label,
        groupLabel: idx === 0 ? g.label : undefined,
      })),
    ),
  ]);

  /** Runtime topics travel as the converter's appendix — it takes them as HTML
   *  strings, which is what they already are, so they need no offscreen pass. */
  const exportAppendix = $derived<PluginDocEntry[]>(
    htmlItems
      .filter((i) => !i.excludeFromExport)
      .map((i) => ({ name: i.exportName ?? i.label, doc: i.html })),
  );

  async function exportAs(format: 'md' | 'html') {
    if (exporting) return;
    exporting = true;
    exportMode = true;
    await tick(); await tick();

    try {
      if (!offscreenEl) return;
      const found = new Map<string, HTMLElement>();
      for (const el of offscreenEl.querySelectorAll<HTMLElement>('[data-section]')) {
        if (el.dataset.section) found.set(el.dataset.section, el);
      }
      const present = exportOrder.filter((s) => found.has(s.id));

      if (format === 'md') {
        const entries: SectionEntry[] = present.map((s) => ({
          id: s.id, label: s.label, el: found.get(s.id)!,
        }));
        pendingExport = {
          content: buildReadme(entries, exportAppendix, productName),
          defaultName: `${exportBase}.md`,
        };
      } else {
        const entries: HtmlSectionEntry[] = present.map((s) => ({
          id: s.id, label: s.label, groupLabel: s.groupLabel, html: found.get(s.id)!.innerHTML,
        }));
        pendingExport = {
          content: buildHtmlExport(entries, exportAppendix, productName),
          defaultName: `${exportBase}.html`,
        };
      }
    } finally {
      exportMode = false;
      exporting = false;
    }
  }

  async function finishExport(filePath: string) {
    const held = pendingExport;
    pendingExport = null;
    if (!held) return;
    const fileName = filePath.split(/[\\/]/).pop() ?? filePath;
    try {
      await fsWriteTextFile(filePath, held.content);
      notificationsStore.add(`${productName} documentation exported`, fileName, 'success');
    } catch (e) {
      notificationsStore.add('Documentation export failed', String(e), 'error');
    }
  }

  const exportItems: DropdownItem[] = [
    { kind: 'item', id: 'md', label: 'Markdown README', meta: '.md', onclick: () => void exportAs('md') },
    { kind: 'item', id: 'html', label: 'Styled HTML', meta: '.html', onclick: () => void exportAs('html') },
  ];
</script>

{#if extracting || exportMode}
  <div bind:this={offscreenEl} class="docs-offscreen" aria-hidden="true">
    {#each orderedSections as s (s.id)}
      {#if sections[s.id]}
        {@const Comp = sections[s.id]}
        <div data-section={s.id}><Comp /></div>
      {/if}
    {/each}
    <!-- Runtime topics: indexed like any other section, so a search finds them
         even before their nav group has ever been opened. -->
    {#each htmlItems as item (item.id)}
      <div data-section={item.id}>{@html item.html}</div>
    {/each}
  </div>
{/if}

<Modal {onClose} {width} {height} padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if headerIcon}{@const HeaderIcon = headerIcon}<HeaderIcon size={14} />{/if}
      <span class="modal-title">{title}</span>
      {#snippet actions()}
        <!-- `fixed`, like every other menu opened from a title bar: an absolutely
             positioned one is clipped by the modal's own overflow. -->
        <Dropdown items={exportItems} position="fixed" direction="down" width="190px">
          {#snippet trigger({ toggle })}
            <Button
              variant="icon"
              size="xs"
              tooltip="Export this documentation"
              ariaLabel="Export documentation"
              disabled={exporting}
              onclick={toggle}
            >
              {#snippet iconStart()}
                {#if exporting}<Spinner size={13} />{:else}<FileDown size={14} />{/if}
              {/snippet}
            </Button>
          {/snippet}
        </Dropdown>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="docs-body">
    <nav class="docs-nav">
      <div class="docs-search-wrap">
        <SearchBar
          bind:query={searchQuery} bind:regex={searchRegex} {regexInvalid}
          current={contentMarks.length > 0 ? currentMarkIdx + 1 : 0} total={contentMarks.length}
          placeholder="Search docs…" ariaLabel="Search documentation"
          oninput={onSearchInput} onClear={clearSearch} onNext={nextMatch} onPrev={prevMatch} />
      </div>

      {#if searchActive && matchingIds.size === 0}
        <p class="search-empty">{regexInvalid ? 'Invalid regex pattern' : 'No matches'}</p>
      {/if}

      {#each topItems as item (item.id)}
        {@const Icon = item.icon}
        {#if !searchActive || matchingIds.has(item.id)}
          <button class="nav-item" class:active={activeSection === item.id} onclick={() => selectSection(item.id)}>
            {#if Icon}<Icon size={13} />{/if}
            {#if searchActive && labelMatchIds.has(item.id)}
              <span class="nav-label">{@html highlightLabel(item.label, compiledQuery)}</span>
            {:else}<span class="nav-label">{item.label}</span>{/if}
          </button>
        {/if}
      {/each}

      {#each renderGroups as group (group.id)}
        {@const GroupIcon = group.icon}
        {@const groupHits = group.items.filter((i) => matchingIds.has(i.id))}
        {@const groupVisible = !searchActive || groupHits.length > 0}
        {@const expanded = searchActive ? groupHits.length > 0 : groupOpen[group.id] === true}
        {#if groupVisible && group.items.length > 0}
          <button class="nav-group-header" onclick={() => { if (!searchActive) groupOpen[group.id] = !groupOpen[group.id]; }}>
            {#if GroupIcon}<GroupIcon size={13} />{/if}
            <span>{group.label}</span>
            {#if searchActive && groupHits.length > 0}<span class="nav-group-count">{groupHits.length}</span>{/if}
            <span class="nav-group-chevron" class:open={expanded}><ChevronRight size={11} /></span>
          </button>
          {#if expanded}
            <div transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>
              {#each group.items as item (item.id)}
                {@const Icon = item.icon}
                {#if !searchActive || matchingIds.has(item.id)}
                  <button
                    class="nav-item nav-item-child"
                    class:active={activeSection === item.id}
                    class:muted={item.muted}
                    use:tooltip={item.tooltip}
                    onclick={() => selectSection(item.id)}
                  >
                    {#if Icon}<Icon size={12} />{/if}
                    {#if searchActive && labelMatchIds.has(item.id)}
                      <span class="nav-label">{@html highlightLabel(item.label, compiledQuery)}</span>
                    {:else}<span class="nav-label">{item.label}</span>{/if}
                    {#if item.pill}<span class="nav-pill">{item.pill}</span>{/if}
                  </button>
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      {/each}
    </nav>

    <div class="docs-content">
      {#key activeSection}
        <PluginDocBlock bind:innerEl={contentEl}>
          {#snippet children()}
            {#if sections[activeSection]}
              {@const Active = sections[activeSection]}
              <Active />
            {:else if activeHtml}
              {@html activeHtml.html}
            {:else if activeSection}
              <!-- Reachable when a runtime topic disappears under the reader —
                   a plugin unloaded while its page was open. -->
              <p class="topic-gone">This topic is not available.</p>
            {/if}
          {/snippet}
        </PluginDocBlock>
      {/key}
    </div>
  </div>
</Modal>

<!-- After the modal in DOM order, so the picker stacks above it: both sit on the
     same z-layer and the later one wins. -->
{#if pendingExport}
  <FileExplorerModal
    mode="save"
    title="Export documentation"
    initialFilename={pendingExport.defaultName}
    onConfirm={(path) => void finishExport(path)}
    onCancel={() => { pendingExport = null; }}
  />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .docs-offscreen { position: fixed; left: -9999px; top: -9999px; visibility: hidden; pointer-events: none; width: 800px; }

  .docs-body { display: flex; flex: 1; height: 100%; overflow: hidden; background: var(--bg-elevated); padding: 4px; }

  .docs-search-wrap { margin: 8px 8px 4px; }
  .search-empty { font-size: var(--font-size-xs); color: var(--text-muted); padding: 12px; margin: 0; text-align: center; font-style: italic; }

  .docs-nav {
    flex-shrink: 0; width: 230px; background: var(--bg-base); border-radius: 12px;
    margin-right: 4px; padding: 8px 0; overflow-y: auto;
    display: flex; flex-direction: column; gap: 1px;
  }
  .nav-item {
    display: flex; align-items: center; gap: 7px; padding: 6px 14px;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-muted); font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    text-align: left; width: 100%; transition: background var(--transition-fast), color var(--transition-fast);
  }
  .nav-item:hover { background: rgba(255,255,255,0.04); color: var(--text-secondary); }
  .nav-item.active { background: rgba(77,120,204,0.14); color: var(--accent); font-weight: 500; border-right: 2px solid var(--accent); }
  .nav-label { flex: 1; }

  .nav-group-header {
    display: flex; align-items: center; gap: 7px; padding: 6px 14px;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-muted); font-family: var(--font-ui-sans); font-size: var(--font-size-sm);
    text-align: left; width: 100%; margin-top: 6px;
    border-top: 1px solid var(--border-subtle); padding-top: 10px;
    transition: color var(--transition-fast);
  }
  .nav-group-header:hover { color: var(--text-secondary); }
  .nav-group-header span:first-of-type { flex: 1; }
  .nav-group-count {
    display: inline-flex; align-items: center; justify-content: center;
    min-width: 18px; height: 14px; padding: 0 5px;
    background: var(--accent-subtle); color: var(--accent);
    font-size: var(--font-size-3xs); font-weight: 700; border-radius: var(--radius-sm); margin-right: 4px;
  }
  .nav-group-chevron { display: flex; align-items: center; transition: transform 150ms ease; }
  .nav-group-chevron.open { transform: rotate(90deg); }
  .nav-item-child { padding-left: 28px; font-size: var(--font-size-xs); }
  .nav-item-child.active { background: rgba(77,120,204,0.10); }

  /* A muted runtime topic stays readable — it is still documentation, it just
     won't be in the exported file, and the strike-through is what says so. */
  .nav-item.muted .nav-label {
    color: var(--text-disabled);
    font-style: italic;
    text-decoration: line-through;
    text-decoration-color: var(--text-disabled);
    opacity: 0.85;
  }
  .nav-pill {
    margin-left: auto;
    padding: 1px 6px;
    font-size: var(--font-size-3xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
    background: var(--bg-hover);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
  }

  .docs-content {
    /* `min-width: 0` is essential: as a flex child it would otherwise grow to its
       widest content (a long table cell / unbreakable code token), pushing the
       whole prose column past the panel — every paragraph then reads as one
       off-window line. Clamping it to the available width lets the content wrap. */
    flex: 1; min-width: 0; overflow-y: auto; background: var(--bg-base); border-radius: 12px;
    scrollbar-width: thin; scrollbar-color: var(--border) transparent;
  }
  .docs-content::-webkit-scrollbar { width: 5px; }
  .docs-content::-webkit-scrollbar-thumb { background: var(--border); border-radius: var(--radius-sm); }

  .topic-gone { color: var(--text-muted); font-style: italic; margin-top: 24px; }

  :global(.nav-label mark) { background: color-mix(in srgb, var(--accent) 35%, transparent); border-radius: 2px; padding: 0 1px; }
  :global(mark.docs-match) {
    background: color-mix(in srgb, var(--warning, #e8a33d) 40%, transparent); border-radius: 2px; padding: 0 1px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--warning, #e8a33d) 30%, transparent);
  }
  :global(mark.docs-match.current) { background: var(--warning, #e8a33d); color: var(--bg-base); }

  /* The doc authoring vocabulary (`.doc-lead`, `.callout`, `.step-list`,
     `.feature-grid`, `.badge`, `.hint`, …) lives in `PluginDocBlock` together
     with the typography it belongs to: every surface that renders authored doc
     HTML — this panel and the Marketplace detail pane — goes through that
     widget, so the classes are defined exactly once. */
</style>
