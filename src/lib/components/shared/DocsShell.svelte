<script module lang="ts">
  import type { Component } from 'svelte';

  export interface DocsNavItem { id: string; label: string; icon?: Component; }
  export interface DocsNavGroup { id: string; label: string; icon?: Component; items: DocsNavItem[]; }
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
   * Search filters the nav (label + full-text via an offscreen index) and
   * highlights matches in the active section (F3 / Shift+F3 to cycle).
   */
  import { tick, untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { ChevronRight } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SearchBar from './ui/SearchBar.svelte';
  import PluginDocBlock from '$lib/components/shared/internal/PluginDocBlock.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import {
    compileQuery, highlightLabel, textMatches, injectHighlights, clearHighlights,
  } from '$lib/utils/text-search';

  let {
    topItems = [],
    navGroups = [],
    sections,
    onClose,
    title = 'Documentation',
    headerIcon,
    initialSection,
    width = '1040px',
    height = '700px',
  }: {
    topItems?: DocsNavItem[];
    navGroups?: DocsNavGroup[];
    sections: Record<string, Component>;
    onClose: () => void;
    title?: string;
    headerIcon?: Component;
    initialSection?: string;
    width?: string;
    height?: string;
  } = $props();

  let activeSection = $state(initialSection ?? topItems[0]?.id ?? navGroups[0]?.items[0]?.id ?? '');
  let groupOpen = $state<Record<string, boolean>>(
    Object.fromEntries(navGroups.map((g, i) => [g.id, i === 0])),
  );

  const orderedSections = $derived([
    ...topItems.map((i) => ({ id: i.id, label: i.label })),
    ...navGroups.flatMap((g) => g.items.map((i) => ({ id: i.id, label: i.label }))),
  ]);

  function selectSection(id: string) {
    activeSection = id;
    for (const g of navGroups) {
      if (g.items.some((it) => it.id === id)) { groupOpen[g.id] = true; break; }
    }
  }

  // ── Search ──────────────────────────────────────────────────────────────
  let searchQuery = $state('');
  let searchRegex = $state(false);
  let searchEl = $state<HTMLElement | null>(null);
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
      if (searchEl) {
        for (const s of orderedSections) {
          const el = searchEl.querySelector<HTMLElement>(`[data-section="${cssEscape(s.id)}"]`);
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
    for (const g of navGroups) if (g.items.some((i) => matches.has(i.id))) groupOpen[g.id] = true;
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
</script>

{#if extracting}
  <div bind:this={searchEl} class="docs-offscreen" aria-hidden="true">
    {#each orderedSections as s (s.id)}
      {#if sections[s.id]}
        {@const Comp = sections[s.id]}
        <div data-section={s.id}><Comp /></div>
      {/if}
    {/each}
  </div>
{/if}

<Modal {onClose} {width} {height} padBody={false} ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if headerIcon}{@const HeaderIcon = headerIcon}<HeaderIcon size={14} />{/if}
      <span class="modal-title">{title}</span>
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

      {#each navGroups as group (group.id)}
        {@const GroupIcon = group.icon}
        {@const groupHits = group.items.filter((i) => matchingIds.has(i.id))}
        {@const groupVisible = !searchActive || groupHits.length > 0}
        {@const expanded = searchActive ? groupHits.length > 0 : groupOpen[group.id]}
        {#if groupVisible}
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
                  <button class="nav-item nav-item-child" class:active={activeSection === item.id} onclick={() => selectSection(item.id)}>
                    {#if Icon}<Icon size={12} />{/if}
                    {#if searchActive && labelMatchIds.has(item.id)}
                      <span class="nav-label">{@html highlightLabel(item.label, compiledQuery)}</span>
                    {:else}<span class="nav-label">{item.label}</span>{/if}
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
            {/if}
          {/snippet}
        </PluginDocBlock>
      {/key}
    </div>
  </div>
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

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
    font-size: 9px; font-weight: 700; border-radius: var(--radius-sm); margin-right: 4px;
  }
  .nav-group-chevron { display: flex; align-items: center; transition: transform 150ms ease; }
  .nav-group-chevron.open { transform: rotate(90deg); }
  .nav-item-child { padding-left: 28px; font-size: var(--font-size-xs); }
  .nav-item-child.active { background: rgba(77,120,204,0.10); }

  .docs-content {
    flex: 1; overflow-y: auto; background: var(--bg-base); border-radius: 12px;
    scrollbar-width: thin; scrollbar-color: var(--border) transparent;
  }
  .docs-content::-webkit-scrollbar { width: 5px; }
  .docs-content::-webkit-scrollbar-thumb { background: var(--border); border-radius: var(--radius-sm); }

  :global(.nav-label mark) { background: color-mix(in srgb, var(--accent) 35%, transparent); border-radius: 2px; padding: 0 1px; }
  :global(mark.docs-match) {
    background: color-mix(in srgb, var(--warning, #e8a33d) 40%, transparent); border-radius: 2px; padding: 0 1px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--warning, #e8a33d) 30%, transparent);
  }
  :global(mark.docs-match.current) { background: var(--warning, #e8a33d); color: var(--bg-base); }

  /* ── Doc authoring utilities (lead paragraph + callout) ─────────────── */
  .docs-content :global(.doc-lead) {
    font-size: 13px; color: var(--text-secondary);
    border-left: 3px solid var(--accent); padding: 8px 0 8px 14px;
    margin-bottom: 18px; line-height: 1.75;
  }
  .docs-content :global(.callout) {
    display: flex; gap: 8px; background: var(--bg-overlay);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    padding: 10px 14px; margin: 12px 0; color: var(--text-secondary);
    font-size: var(--font-size-sm); line-height: 1.6;
  }
  .docs-content :global(.callout.accent) { border-left: 3px solid var(--accent); }

  /* Numbered visual steps */
  .docs-content :global(ol.step-list) {
    padding-left: 0; list-style: none; counter-reset: step;
    display: flex; flex-direction: column; gap: 6px; margin: 12px 0;
  }
  .docs-content :global(ol.step-list > li) {
    counter-increment: step;
    display: flex; align-items: flex-start; gap: 12px;
    padding: 9px 14px 9px 12px;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm); color: var(--text-secondary); line-height: 1.6;
  }
  .docs-content :global(ol.step-list > li::before) {
    content: counter(step); flex-shrink: 0;
    width: 20px; height: 20px; margin-top: 1px;
    background: var(--accent); color: #fff; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-size: 10px; font-weight: 700;
  }
</style>
