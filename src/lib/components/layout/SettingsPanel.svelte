<script lang="ts">
  import {
    Monitor, GitBranch, Code, Keyboard, Github, TicketCheck, FolderGit2, ChevronRight, Sparkles, GitMerge, FlaskConical, Database, Layers, BarChart2, ShieldCheck, FolderX, Terminal, GitPullRequest, Settings, Workflow, ExternalLink, Link2, Boxes, Command, Store,
  } from 'lucide-svelte';
  import { fade } from 'svelte/transition';
  import { tick, untrack } from 'svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import {
    compileQuery, highlightLabel, textMatches,
    injectHighlights, clearHighlights,
  } from '$lib/utils/text-search';
  import AppearanceSection            from './settings/AppearanceSection.svelte';
  import GraphSection                 from './settings/GraphSection.svelte';
  import DiffSection                  from './settings/DiffSection.svelte';
  import KeybindingsSection           from './settings/KeybindingsSection.svelte';
  import GitSection                   from './settings/GitSection.svelte';
  import IssueTrackersSection         from './settings/IssueTrackersSection.svelte';
  import RepositorySection            from './settings/RepositorySection.svelte';
  import ProjectIssueTrackerSection   from './settings/ProjectIssueTrackerSection.svelte';
  import AnimationsSection            from './settings/AnimationsSection.svelte';
  import KeystrokesSection             from './settings/KeystrokesSection.svelte';
  import GitFlowSection               from './settings/GitFlowSection.svelte';
  import ExperimentalSection          from './settings/ExperimentalSection.svelte';
  import CacheSection                 from './settings/CacheSection.svelte';
  import IdeSection                   from './settings/IdeSection.svelte';
  import TerminalsSection              from './settings/TerminalsSection.svelte';
  import StatsSection                 from './settings/StatsSection.svelte';
  import RecoverySection              from './settings/RecoverySection.svelte';
  import MissingProjectsSection       from './settings/MissingProjectsSection.svelte';
  import GitCliSection                 from './settings/GitCliSection.svelte';
  import MrSection                     from './settings/MrSection.svelte';
  import PipelinesSection               from './settings/PipelinesSection.svelte';
  import ExternalIntegrationsSection    from './settings/ExternalIntegrationsSection.svelte';
  import ProjectGitFlowSection          from './settings/ProjectGitFlowSection.svelte';
  import DeepLinkSection                from './settings/DeepLinkSection.svelte';
  import StudioSection                  from './settings/StudioSection.svelte';
  import MarketplaceSection              from './settings/MarketplaceSection.svelte';

  let { onClose, onOpenThemeEditor }: {
    onClose: () => void;
    onOpenThemeEditor: () => void;
  } = $props();

  type Section = 'appearance' | 'animations' | 'keystrokes' | 'graph' | 'diff' | 'git' | 'git-cli' | 'issue-trackers' | 'repository' | 'project-issue-tracker' | 'project-gitflow' | 'project-ext-integrations' | 'keybindings' | 'gitflow' | 'experimental' | 'cache' | 'ide' | 'terminals' | 'stats' | 'recovery' | 'missing-projects' | 'mr' | 'pipelines' | 'deep-link' | 'studio' | 'marketplace';
  let activeSection = $state<Section>('appearance');

  const sectionComponents: Record<Section, any> = {
    appearance:                  AppearanceSection,
    animations:                  AnimationsSection,
    keystrokes:                  KeystrokesSection,
    graph:                       GraphSection,
    diff:                        DiffSection,
    keybindings:                 KeybindingsSection,
    git:                         GitSection,
    'git-cli':                   GitCliSection,
    'issue-trackers':            IssueTrackersSection,
    repository:                  RepositorySection,
    'project-issue-tracker':     ProjectIssueTrackerSection,
    'project-gitflow':           ProjectGitFlowSection,
    'project-ext-integrations':  ExternalIntegrationsSection,
    gitflow:                     GitFlowSection,
    experimental:                ExperimentalSection,
    cache:                       CacheSection,
    ide:                         IdeSection,
    terminals:                   TerminalsSection,
    stats:                       StatsSection,
    recovery:                    RecoverySection,
    'missing-projects':          MissingProjectsSection,
    mr:                          MrSection,
    pipelines:                   PipelinesSection,
    'deep-link':                 DeepLinkSection,
    studio:                      StudioSection,
    marketplace:                 MarketplaceSection,
  };

  const navGroups: { label: string; items: { id: Section; label: string; icon: any }[] }[] = [
    {
      label: 'Interface',
      items: [
        { id: 'appearance',  label: 'Appearance',  icon: Monitor   },
        { id: 'animations',  label: 'Animations',  icon: Sparkles  },
        { id: 'keystrokes',  label: 'Keyboard Inputs', icon: Command },
        { id: 'graph',       label: 'Graph',        icon: GitBranch },
        { id: 'diff',        label: 'Diff & Stage', icon: Code      },
        { id: 'keybindings', label: 'Keybindings',  icon: Keyboard  },
      ],
    },
    {
      label: 'Git',
      items: [
        { id: 'git-cli',           label: 'Git Executable',   icon: Terminal       },
        { id: 'gitflow',           label: 'Git Flow',         icon: GitMerge       },
        { id: 'mr',                label: 'Merge Requests',   icon: GitPullRequest },
        { id: 'recovery',          label: 'Recovery',         icon: ShieldCheck    },
        { id: 'missing-projects',  label: 'Missing Projects', icon: FolderX        },
        { id: 'experimental',      label: 'Experimental',     icon: FlaskConical   },
      ],
    },
    // ── Tools ────────────────────────────────────────────────────────────────
    // Global, host-wide integrations: pipeline orchestration + the IDE /
    // terminal registries that drive "Open in IDE" / "Open Terminal" across
    // every repo. Per-project overrides live under the Project group below
    // (see ExternalIntegrationsSection for the IDE override and
    // ProjectGitFlowSection for the Git Flow override).
    {
      label: 'Tools',
      items: [
        { id: 'marketplace', label: 'Marketplace',     icon: Store    },
        { id: 'pipelines',   label: 'Pipelines',       icon: Workflow },
        { id: 'ide',         label: 'IDE Integration', icon: Layers   },
        { id: 'terminals',   label: 'Terminals',       icon: Terminal },
        { id: 'studio',      label: 'Studio',          icon: Boxes    },
        { id: 'deep-link',   label: 'Deep Links',      icon: Link2    },
      ],
    },
    {
      label: 'Performance',
      items: [
        { id: 'cache', label: 'Cache', icon: Database },
      ],
    },
    {
      label: 'Access',
      items: [
        { id: 'git',             label: 'Git',            icon: Github          },
        { id: 'issue-trackers',  label: 'Issue Trackers', icon: TicketCheck     },
      ],
    },
    {
      label: 'Project',
      items: [
        { id: 'repository',                label: 'Repository',            icon: FolderGit2     },
        { id: 'project-issue-tracker',     label: 'Issue Tracker',         icon: TicketCheck    },
        { id: 'project-ext-integrations',  label: 'External Integrations', icon: ExternalLink   },
        { id: 'project-gitflow',           label: 'Git Flow',              icon: GitMerge       },
        { id: 'stats',                     label: 'Statistics',            icon: BarChart2      },
      ],
    },
  ];

  // ── Search ──────────────────────────────────────────────────────────────
  let searchQuery   = $state('');
  let searchRegex   = $state(false);
  let searchEl      = $state<HTMLElement | null>(null);
  let contentEl     = $state<HTMLElement | null>(null);

  let matchingIds   = $state<Set<Section>>(new Set());
  let labelMatchIds = $state<Set<Section>>(new Set());
  let contentMarks  = $state<HTMLElement[]>([]);
  let currentMarkIdx = $state(0);
  let extracting    = $state(false);

  const searchActive  = $derived(searchQuery.trim().length > 0);
  const compiledQuery = $derived.by(() =>
    compileQuery(searchQuery.trim(), { regex: searchRegex })
  );
  const regexInvalid = $derived(
    searchRegex && searchQuery.trim().length > 0 && compiledQuery === null
  );

  const allSectionIds: Section[] = navGroups.flatMap(g => g.items.map(i => i.id));

  function cssEscape(s: string): string {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/(["\\])/g, '\\$1');
  }

  // Cache per-section text (excluding pre/code) — built once, reused across
  // queries. Skipping pre/code in the index keeps it consistent with the
  // highlight injector so a section can never appear in `matchingIds` while
  // showing zero `<mark>` tags in its body.
  const textCache = new Map<Section, string>();

  function extractText(root: HTMLElement): string {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        let p = node.parentElement;
        while (p && p !== root) {
          const t = p.tagName;
          if (t === 'PRE' || t === 'CODE' || t === 'SCRIPT' || t === 'STYLE') {
            return NodeFilter.FILTER_REJECT;
          }
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
      await tick();
      await tick();
      if (searchEl) {
        for (const id of allSectionIds) {
          const el = searchEl.querySelector<HTMLElement>(`[data-section="${cssEscape(id)}"]`);
          if (el) textCache.set(id, extractText(el));
        }
      }
      extracting = false;
      _cacheBuilding = null;
    })();
    return _cacheBuilding;
  }

  async function doSearch() {
    if (!searchActive) {
      matchingIds = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    await ensureCache();
    const re = compiledQuery;
    if (!re) {
      matchingIds = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    const matches = new Set<Section>();
    const labels  = new Set<Section>();
    for (const g of navGroups) {
      const groupLabelHit = textMatches(g.label, re);
      for (const item of g.items) {
        const labelHit = textMatches(item.label, re) || groupLabelHit;
        if (labelHit) labels.add(item.id);
        if (labelHit || textMatches(textCache.get(item.id) ?? '', re)) matches.add(item.id);
      }
    }
    matchingIds   = matches;
    labelMatchIds = labels;

    if (!matches.has(activeSection)) {
      const first = allSectionIds.find(id => matches.has(id));
      if (first) activeSection = first;
    }

    await applyContentHighlights();
  }

  let _searchTimer: ReturnType<typeof setTimeout> | null = null;
  function onSearchInput() {
    if (_searchTimer) clearTimeout(_searchTimer);
    _searchTimer = setTimeout(doSearch, 120);
  }

  $effect(() => {
    searchRegex; // track
    if (searchActive) doSearch();
  });

  function clearSearch() {
    searchQuery   = '';
    matchingIds   = new Set();
    labelMatchIds = new Set();
    if (contentEl) {
      clearHighlights(contentEl, 'settings-match');
      contentMarks = [];
      currentMarkIdx = 0;
    }
  }

  async function applyContentHighlights() {
    await tick();
    await tick();
    if (!contentEl || !contentEl.isConnected) return;
    clearHighlights(contentEl, 'settings-match');
    contentMarks = [];
    currentMarkIdx = 0;
    const re = compiledQuery;
    if (!re || !searchActive) return;
    const marks = injectHighlights(contentEl, re, { className: 'settings-match' });
    contentMarks = marks;
    if (marks.length > 0) {
      marks[0].classList.add('current');
      marks[0].scrollIntoView({ block: 'center', behavior: 'auto' });
    }
  }

  // Re-apply highlights on section change (untrack so we don't fire per-keystroke).
  $effect(() => {
    activeSection;
    untrack(() => { applyContentHighlights(); });
  });

  // Pre-build the cache when the modal opens.
  let _prebuildStarted = false;
  $effect(() => {
    if (_prebuildStarted) return;
    _prebuildStarted = true;
    setTimeout(() => { ensureCache(); }, 80);
  });

  function gotoMark(idx: number) {
    if (contentMarks.length === 0) return;
    const wrapped = ((idx % contentMarks.length) + contentMarks.length) % contentMarks.length;
    contentMarks.forEach(m => m.classList.remove('current'));
    const target = contentMarks[wrapped];
    target.classList.add('current');
    target.scrollIntoView({ block: 'center', behavior: 'smooth' });
    currentMarkIdx = wrapped;
  }

  function jumpSection(dir: 1 | -1) {
    const order = allSectionIds.filter(id => matchingIds.has(id));
    if (order.length === 0) return;
    const i = order.indexOf(activeSection);
    if (i === -1) { activeSection = order[0]; return; }
    activeSection = order[(i + dir + order.length) % order.length];
  }

  function nextMatch() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx + 1);
    else jumpSection(+1);
  }
  function prevMatch() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx - 1);
    else jumpSection(-1);
  }

  $effect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'F3') {
        e.preventDefault();
        if (e.shiftKey) prevMatch(); else nextMatch();
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

<Modal {onClose} width="840px" height="540px" padBody={false} ariaLabel="Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <div class="settings-body">
    <!-- Sidebar nav -->
    <nav class="nav" aria-label="Settings sections">
      <div class="nav-search">
        <SearchBar
          bind:query={searchQuery}
          bind:regex={searchRegex}
          regexInvalid={regexInvalid}
          current={contentMarks.length > 0 ? currentMarkIdx + 1 : 0}
          total={contentMarks.length}
          placeholder="Search settings…"
          ariaLabel="Search settings"
          oninput={onSearchInput}
          onClear={clearSearch}
          onNext={nextMatch}
          onPrev={prevMatch}
        />
      </div>

      {#if searchActive && matchingIds.size === 0}
        <p class="search-empty">
          {regexInvalid ? 'Invalid regex pattern' : 'No matches'}
        </p>
      {/if}

      {#each navGroups as group}
        {@const groupHits = group.items.filter(i => matchingIds.has(i.id))}
        {#if !searchActive || groupHits.length > 0}
          <div class="nav-group-label">
            {group.label}
            {#if searchActive && groupHits.length > 0}
              <span class="nav-group-count">{groupHits.length}</span>
            {/if}
          </div>
          {#each group.items as item}
            {@const ItemIcon = item.icon}
            {#if !searchActive || matchingIds.has(item.id)}
              <button
                class="nav-item"
                class:active={activeSection === item.id}
                onclick={() => (activeSection = item.id)}
              >
                <ItemIcon size={13} />
                {#if searchActive && labelMatchIds.has(item.id)}
                  <span>{@html highlightLabel(item.label, compiledQuery)}</span>
                {:else}
                  <span>{item.label}</span>
                {/if}
                {#if activeSection === item.id}
                  <ChevronRight size={11} class="nav-arrow" />
                {/if}
              </button>
            {/if}
          {/each}
        {/if}
      {/each}
    </nav>

    <!-- Content area — fade on section switch -->
    {#key activeSection}
      {@const SectionComponent = sectionComponents[activeSection]}
      <div class="content" bind:this={contentEl} in:fade={{ duration: animStore.dFast }}>
        <SectionComponent
          {...(activeSection === 'appearance' ? { onOpenThemeEditor } : {})}
        />
      </div>
    {/key}
  </div>
</Modal>

<!-- Hidden offscreen container — mounted briefly during cache extraction. -->
{#if extracting}
  <div bind:this={searchEl} class="search-offscreen" aria-hidden="true">
    {#each navGroups as group}
      {#each group.items as item}
        {@const SectionComponent = sectionComponents[item.id]}
        <div data-section={item.id}>
          <SectionComponent
            {...(item.id === 'appearance' ? { onOpenThemeEditor: () => {} } : {})}
          />
        </div>
      {/each}
    {/each}
  </div>
{/if}

<style>
  /* ── Shell ──────────────────────────────────────────────────────── */
  /* Mirrors the conflict modal layout: bg-elevated reveals as a 4px gap
     around floating bg-base panel cards (sidebar + content). */
  .settings-body {
    display: flex;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
  }

  /* ── Nav ────────────────────────────────────────────────────────── */
  .nav {
    width: 230px;
    flex-shrink: 0;
    background: var(--bg-base);
    border-radius: 12px;
    margin-right: 4px;
    padding: 8px 0 16px;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .nav-search {
    margin: 0 8px 6px;
  }

  .search-empty {
    font-size: 11px;
    color: var(--text-muted);
    padding: 12px 14px;
    margin: 0;
    text-align: center;
    font-style: italic;
  }

  .nav-group-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-disabled);
    text-transform: uppercase;
    letter-spacing: 0.7px;
    padding: 10px 14px 4px;
  }
  .nav-group-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 13px;
    padding: 0 4px;
    background: var(--accent-subtle);
    color: var(--accent);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0;
    border-radius: var(--radius-sm);
    text-transform: none;
  }

  /* Highlight inside nav labels */
  .nav-item :global(span mark) {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }

  /* Highlights injected into the active section's content */
  :global(mark.settings-match) {
    background: color-mix(in srgb, var(--warning, #e8a33d) 40%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--warning, #e8a33d) 30%, transparent);
  }
  :global(mark.settings-match.current) {
    background: var(--warning, #e8a33d);
    color: var(--bg-base);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--warning, #e8a33d) 50%, transparent);
  }

  /* Offscreen search container */
  .search-offscreen {
    position: fixed;
    left: -9999px;
    top: -9999px;
    visibility: hidden;
    pointer-events: none;
    width: 600px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 14px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
    position: relative;
  }
  .nav-item:hover:not(.active) { background: var(--bg-hover); color: var(--text-primary); }
  .nav-item.active {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
  }
  .nav-item span { flex: 1; }

  :global(.nav-arrow) { opacity: 0.55; flex-shrink: 0; }

  /* ── Content area ───────────────────────────────────────────────── */
  .content {
    flex: 1;
    min-height: 0;
    background: var(--bg-base);
    border-radius: 12px;
    padding: 22px 24px 32px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .content > :global(*) { flex-shrink: 0; }

  /* ── Section header ─────────────────────────────────────────────── */
  .content :global(.section-header) { margin-bottom: 4px; }
  .content :global(.section-header h2) {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
  }
  .content :global(.section-header p) {
    font-size: 11px;
    color: var(--text-muted);
    margin: 0;
    line-height: 1.5;
  }

  /* ── Card ───────────────────────────────────────────────────────── */
  .content :global(.card) {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .content :global(.card-section-title) {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 10px 14px 8px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
  }

  .content :global(.card-row) {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 11px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .content :global(.card-row:last-child) { border-bottom: none; }
  .content :global(.card-row:hover) { background: rgba(255,255,255,0.015); }

  .content :global(.card-row-note) {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.55;
    padding: 8px 14px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .content :global(.card-row-column) {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 8px 14px 12px;
  }

  /* ── Row layout ─────────────────────────────────────────────────── */
  /* Title / description / control rules live in the `<FormRow>` widget
     (`fr-row`/`fr-title`/`fr-desc`/`fr-control` classes).  Only the
     legacy `.inline-control` helper used by AppearanceSection's font
     slider remains here. */
  .content :global(.inline-control) { gap: 10px; }

  /* ── Inputs ─────────────────────────────────────────────────────── */
  .content :global(.text-input),
  .content :global(.select-input) {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 5px 8px;
    min-width: 160px;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
    outline: none;
  }
  .content :global(.text-input:focus),
  .content :global(.select-input:focus) {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(77,120,204,0.15);
  }
  .content :global(.text-input.narrow) { min-width: 80px; }

  .content :global(.input-with-addon) {
    display: flex;
    align-items: center;
    gap: 0;
    flex: 1;
  }
  .content :global(.input-with-addon .text-input) {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    border-right: none;
    flex: 1;
    min-width: 0;
  }
  .content :global(.addon-btn) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: pointer;
    color: var(--text-muted);
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .content :global(.addon-btn:hover) { background: var(--bg-hover); color: var(--text-primary); }

  .content :global(.template-textarea) {
    width: 100%;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.6;
    padding: 8px 10px;
    resize: vertical;
    transition: border-color var(--transition-fast);
    box-sizing: border-box;
  }
  .content :global(.template-textarea:focus) { outline: none; border-color: var(--border-focus); }
  .content :global(.template-textarea::placeholder) { color: var(--text-disabled); }

  /* ── Slider ─────────────────────────────────────────────────────── */
  .content :global(.slider) {
    flex: 1;
    accent-color: var(--accent);
    cursor: pointer;
    height: 4px;
  }
  .content :global(.value-chip) {
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    min-width: 42px;
    text-align: center;
    flex-shrink: 0;
  }

  /* ── Radio group ────────────────────────────────────────────────── */
  .content :global(.radio-group) { display: flex; gap: 6px; }
  .content :global(.radio-option) {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
    color: var(--text-secondary);
    background: var(--bg-input);
    transition: all var(--transition-fast);
    user-select: none;
  }
  .content :global(.radio-option input) { display: none; }
  .content :global(.radio-option:hover) { border-color: var(--border-focus); color: var(--text-primary); }
  .content :global(.radio-option.selected) {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
  }

  /* ── Info box ───────────────────────────────────────────────────── */
  .content :global(.info-box) {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 14px;
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.55;
  }
  .content :global(.info-box svg) { flex-shrink: 0; margin-top: 1px; opacity: 0.7; }

  /* ── Buttons ────────────────────────────────────────────────────── */
  .content :global(.btn-primary) {
    padding: 6px 18px;
    background: var(--accent);
    color: var(--text-on-accent);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: filter var(--transition-fast), opacity var(--transition-fast);
  }
  .content :global(.btn-primary:hover:not(:disabled)) { filter: brightness(1.12); }
  .content :global(.btn-primary:disabled) { opacity: 0.45; cursor: not-allowed; }

  .content :global(.btn-ghost) {
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .content :global(.btn-ghost:hover) { background: var(--bg-hover); color: var(--text-primary); }

  .content :global(.btn-ghost-danger) {
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .content :global(.btn-ghost-danger:hover) {
    background: var(--error-subtle);
    color: var(--error);
    border-color: var(--error);
  }

  .content :global(.btn-ghost-sm) {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 11px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: all var(--transition-fast);
  }
  .content :global(.btn-ghost-sm:hover) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border-focus);
  }

  /* ── Icon buttons ───────────────────────────────────────────────── */
  .content :global(.icon-btn) {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
    flex-shrink: 0;
  }
  .content :global(.icon-btn:hover) { background: var(--bg-hover); color: var(--text-primary); }
  .content :global(.icon-btn.danger:hover) { background: var(--error-subtle); color: var(--error); }

  /* ── Empty state ────────────────────────────────────────────────── */
  .content :global(.empty-state) {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 36px 20px;
    color: var(--text-disabled);
    font-size: 12px;
    background: var(--bg-elevated);
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
    text-align: center;
  }
  .content :global(.empty-hint) { font-size: 11px; color: var(--text-muted); max-width: 280px; line-height: 1.5; }

  .content :global(.loading-dots) { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }

  /* ── Form ───────────────────────────────────────────────────────── */
  .content :global(.form-error) {
    font-size: 11px;
    color: var(--error);
    margin: 0;
    padding: 0 2px;
  }
  .content :global(.form-actions) {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .content :global(.saved-label) {
    font-size: 11px;
    color: var(--success, #6aab73);
  }

  /* ── Inline code ────────────────────────────────────────────────── */
  .content :global(code) {
    font-family: var(--font-code);
    font-size: 10.5px;
    color: var(--text-secondary);
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
</style>
