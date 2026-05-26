<script lang="ts">
  import { tick, untrack, onMount }   from 'svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { slide, fly }     from 'svelte/transition';
  import { cubicOut }       from 'svelte/easing';
  import { animStore }      from '$lib/stores/animations.svelte';
  import { fsWriteTextFile } from '$lib/ipc/fs';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import FilePickerModal     from './FilePickerModal.svelte';
  import Modal               from './Modal.svelte';
  import ModalHeader         from './ModalHeader.svelte';
  import SearchBar           from './ui/SearchBar.svelte';
  import PluginDocBlock      from './internal/PluginDocBlock.svelte';
  import { tooltip }          from '$lib/actions/tooltip';
  import { listPluginInfo }  from '$lib/ipc/plugin';
  import type { PluginInfo } from '$lib/types/plugin';
  import {
    compileQuery, highlightLabel, textMatches,
    injectHighlights, clearHighlights,
  } from '$lib/utils/text-search';
  import {
    buildReadme, buildHtmlExport,
    type SectionEntry, type HtmlSectionEntry, type PluginDocEntry,
  } from '$lib/utils/docs-export';
  import {
    BookOpen, GitBranch, GitCommitHorizontal, GitMerge, Layers, Zap, Keyboard,
    Package, TerminalSquare, Loader, ChevronRight,
    Bell, FolderGit2, Workflow,
    GitPullRequest, Search, FolderPlus, Download, Settings, TicketCheck,
    FileDown, StickyNote, FolderTree, History, Bug, BarChart2,
    Monitor, Database, Shield, ShieldCheck, Cloud, Tag, Share2, FolderX,
    Palette, Link2, Store,
  } from 'lucide-svelte';

  // ── Section components ───────────────────────────────────────────────────────
  import GettingStarted from './docs/GettingStarted.svelte';
  import InitRepo       from './docs/InitRepo.svelte';
  import GitGraph       from './docs/GitGraph.svelte';
  import StageCommit    from './docs/StageCommit.svelte';
  import Branches       from './docs/Branches.svelte';
  import Submodules     from './docs/Submodules.svelte';
  import GitFlow        from './docs/GitFlow.svelte';
  import Terminal       from './docs/Terminal.svelte';
  import CommandPalette from './docs/CommandPalette.svelte';
  import Shortcuts      from './docs/Shortcuts.svelte';
  import BackgroundJobs from './docs/BackgroundJobs.svelte';
  import Notifications  from './docs/Notifications.svelte';
  import Pipelines      from './docs/Pipelines.svelte';
  import MergeRequests  from './docs/MergeRequests.svelte';
  import CloneRepo            from './docs/CloneRepo.svelte';
  import RepoBrowser          from './docs/RepoBrowser.svelte';
  import PluginDevBasics      from './docs/PluginDevBasics.svelte';
  import PluginDevHooks       from './docs/PluginDevHooks.svelte';
  import PluginDevApiCore     from './docs/PluginDevApiCore.svelte';
  import PluginDevApiUI       from './docs/PluginDevApiUI.svelte';
  import PluginDevApiJobs     from './docs/PluginDevApiJobs.svelte';
  import PluginDevApiGroups   from './docs/PluginDevApiGroups.svelte';
  import SettingsInterface    from './docs/SettingsInterface.svelte';
  import SettingsPerformance  from './docs/SettingsPerformance.svelte';
  import SettingsAccess       from './docs/SettingsAccess.svelte';
  import SettingsProject      from './docs/SettingsProject.svelte';
  import PipelinesLocal       from './docs/PipelinesLocal.svelte';
  import PipelinesCicd        from './docs/PipelinesCicd.svelte';
  import SourceExport         from './docs/SourceExport.svelte';
  import TagsStash            from './docs/TagsStash.svelte';
  import IssuesDocs     from './docs/IssuesDocs.svelte';
  import MergeConflicts from './docs/MergeConflicts.svelte';
  import TicketLinks    from './docs/TicketLinks.svelte';
  import GitNotes       from './docs/GitNotes.svelte';
  import Workspaces       from './docs/Workspaces.svelte';
  import LinkedWorktrees  from './docs/LinkedWorktrees.svelte';
  import Worktrees        from './docs/Worktrees.svelte';
  import FileTree       from './docs/FileTree.svelte';
  import Reflog         from './docs/Reflog.svelte';
  import Recovery       from './docs/Recovery.svelte';
  import MissingProjects from './docs/MissingProjects.svelte';
  import GitExecutable   from './docs/GitExecutable.svelte';
  import GitBisect      from './docs/GitBisect.svelte';
  import Statistics     from './docs/Statistics.svelte';
  import Themes         from './docs/Themes.svelte';
  import Security       from './docs/Security.svelte';
  import DeepLinks      from './docs/DeepLinks.svelte';
  import Marketplace    from './docs/Marketplace.svelte';

  let { onClose }: { onClose: () => void } = $props();

  // ── Nav structure ────────────────────────────────────────────────────────────
  type NavItem  = { id: string; label: string; icon: any };
  type NavGroup = { id: string; label: string; icon: any; items: NavItem[] };

  const topItems: NavItem[] = [
    { id: 'getting-started', label: 'Getting Started',      icon: BookOpen   },
    { id: 'init-repo',       label: 'Initialize Repository', icon: FolderPlus },
    { id: 'clone-repo',      label: 'Clone Repository',      icon: Download   },
    { id: 'workspaces',       label: 'Workspaces',         icon: Layers     },
    { id: 'linked-worktrees', label: 'Linked Worktrees',   icon: Layers     },
    { id: 'repo-browser',     label: 'Repository Browser', icon: Package    },
  ];

  const navGroups: NavGroup[] = [
    {
      id: 'git', label: 'Git', icon: GitBranch,
      items: [
        { id: 'graph',           label: 'Git Graph',        icon: GitBranch          },
        { id: 'stage',           label: 'Stage & Commit',   icon: GitCommitHorizontal },
        { id: 'merge-conflicts', label: 'Merge Conflicts',  icon: GitMerge           },
        { id: 'branches',        label: 'Branches',         icon: GitBranch          },
        { id: 'tags-stash',      label: 'Tags & Stash',     icon: Tag                },
        { id: 'submodules',      label: 'Submodules',       icon: FolderGit2         },
        { id: 'gitflow',         label: 'Git Flow',         icon: GitMerge           },
        { id: 'ticket-links',    label: 'Ticket Links',     icon: TicketCheck        },
        { id: 'git-notes',       label: 'Git Notes',        icon: StickyNote         },
        { id: 'worktrees',       label: 'Worktrees',        icon: Layers             },
        { id: 'file-tree',       label: 'Files',            icon: FolderTree         },
        { id: 'reflog',          label: 'Reflog',           icon: History            },
        { id: 'recovery',        label: 'Recovery Journal', icon: ShieldCheck        },
        { id: 'missing-projects', label: 'Missing Projects',icon: FolderX            },
        { id: 'git-executable',  label: 'Git Executable',   icon: TerminalSquare     },
        { id: 'bisect',          label: 'Git Bisect',       icon: Bug                },
      ],
    },
    {
      id: 'tools', label: 'Tools', icon: Zap,
      items: [
        { id: 'marketplace',     label: 'Marketplace',        icon: Store          },
        { id: 'terminal',        label: 'Terminal',           icon: TerminalSquare },
        { id: 'command-palette', label: 'Command Palette',    icon: Search         },
        { id: 'shortcuts',       label: 'Keyboard Shortcuts', icon: Keyboard       },
        { id: 'statistics',      label: 'Statistics',         icon: BarChart2      },
      ],
    },
    {
      id: 'automation', label: 'Automation', icon: Workflow,
      items: [
        { id: 'jobs',          label: 'Background Jobs',        icon: Loader         },
        { id: 'notifications', label: 'Notifications',          icon: Bell           },
        { id: 'pipelines-local', label: 'Pipelines',              icon: Workflow       },
        { id: 'source-export',   label: 'Source Export plugin',   icon: Share2         },
        { id: 'pipelines-cicd',  label: 'CI / CD',                icon: Cloud          },
        { id: 'mr',            label: 'Pull / Merge Requests',  icon: GitPullRequest },
        { id: 'issues',        label: 'Issues (Linear / Jira)',  icon: TicketCheck    },
        { id: 'security',      label: 'Security Dashboard',     icon: ShieldCheck    },
        { id: 'deep-links',    label: 'Deep Links',             icon: Link2          },
      ],
    },
    {
      id: 'settings', label: 'Settings', icon: Settings,
      items: [
        { id: 'settings-interface',   label: 'Interface & Git',  icon: Monitor   },
        { id: 'settings-performance', label: 'Performance',      icon: Database  },
        { id: 'settings-access',      label: 'Access',           icon: Shield    },
        { id: 'settings-project',     label: 'Project',          icon: FolderGit2 },
        { id: 'themes',               label: 'Themes & Presets', icon: Palette   },
      ],
    },
    {
      id: 'plugin-dev', label: 'Plugin Dev', icon: Package,
      items: [
        { id: 'plugin-dev-basics',   label: 'Basics & Manifest',  icon: Package      },
        { id: 'plugin-dev-hooks',    label: 'Hooks & Constants',  icon: Zap          },
        { id: 'plugin-dev-api-core', label: 'API — Core',         icon: Layers       },
        { id: 'plugin-dev-api-ui',   label: 'API — UI',           icon: Keyboard     },
        { id: 'plugin-dev-api-jobs',   label: 'API — Jobs',              icon: Loader    },
        { id: 'plugin-dev-api-groups', label: 'API — Toolchains',          icon: Database  },
      ],
    },
  ];

  const sectionComponents: Record<string, any> = {
    'getting-started': GettingStarted,
    'init-repo':       InitRepo,
    'graph':           GitGraph,
    'stage':           StageCommit,
    'merge-conflicts': MergeConflicts,
    'ticket-links':    TicketLinks,
    'git-notes':       GitNotes,
    'workspaces':       Workspaces,
    'linked-worktrees': LinkedWorktrees,
    'worktrees':        Worktrees,
    'file-tree':       FileTree,
    'reflog':          Reflog,
    'recovery':        Recovery,
    'missing-projects': MissingProjects,
    'git-executable':  GitExecutable,
    'bisect':          GitBisect,
    'branches':        Branches,
    'submodules':      Submodules,
    'gitflow':         GitFlow,
    'terminal':        Terminal,
    'command-palette': CommandPalette,
    'shortcuts':       Shortcuts,
    'jobs':            BackgroundJobs,
    'notifications':   Notifications,
    'pipelines':       Pipelines,
    'mr':              MergeRequests,
    'issues':                IssuesDocs,
    'clone-repo':            CloneRepo,
    'repo-browser':          RepoBrowser,
    'statistics':            Statistics,
    'settings-interface':    SettingsInterface,
    'settings-performance':  SettingsPerformance,
    'settings-access':       SettingsAccess,
    'settings-project':      SettingsProject,
    'pipelines-local':       PipelinesLocal,
    'pipelines-cicd':        PipelinesCicd,
    'source-export':         SourceExport,
    'tags-stash':            TagsStash,
    'security':              Security,
    'deep-links':            DeepLinks,
    'plugin-dev-basics':     PluginDevBasics,
    'plugin-dev-hooks':      PluginDevHooks,
    'plugin-dev-api-core':   PluginDevApiCore,
    'plugin-dev-api-ui':     PluginDevApiUI,
    'plugin-dev-api-jobs':   PluginDevApiJobs,
    'plugin-dev-api-groups': PluginDevApiGroups,
    'themes':                Themes,
    'marketplace':           Marketplace,
  };

  // ── Group open state ─────────────────────────────────────────────────────────
  let groupOpen = $state<Record<string, boolean>>({
    git: false, tools: false, automation: false, settings: false, 'plugin-dev': false, plugins: false,
  });

  // ── Search ───────────────────────────────────────────────────────────────────
  let searchQuery        = $state('');
  let searchRegex        = $state(false);
  let searchEl           = $state<HTMLElement | null>(null);
  let contentEl          = $state<HTMLElement | null>(null);

  /** IDs of sections whose label or text content matches the current query. */
  let matchingIds        = $state<Set<string>>(new Set());
  /** IDs of sections whose label matches (subset of matchingIds). */
  let labelMatchIds      = $state<Set<string>>(new Set());

  /** `<mark>` elements injected into the active section's content area. */
  let contentMarks       = $state<HTMLElement[]>([]);
  let currentMarkIdx     = $state(0);

  /** True while the offscreen container is mounted to (re)build the cache. */
  let extracting         = $state(false);

  const searchActive = $derived(searchQuery.trim().length > 0);
  const compiledQuery = $derived.by(() =>
    compileQuery(searchQuery.trim(), { regex: searchRegex })
  );
  const regexInvalid = $derived(
    searchRegex && searchQuery.trim().length > 0 && compiledQuery === null
  );

  // ── Per-section text cache (built once, reused across queries) ──────────────
  // Walks each offscreen section once with a TreeWalker that skips pre/code/
  // script/style — matches the same skip-set used by the highlight injector
  // so a section can never appear in `matchingIds` without producing visible
  // highlights inside its content.
  const textCache = new Map<string, string>();

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
      // Two ticks: first to mount offscreen, second for components to render.
      await tick();
      await tick();
      if (searchEl) {
        for (const s of orderedSections) {
          const el = searchEl.querySelector<HTMLElement>(`[data-section="${cssEscape(s.id)}"]`);
          if (el) textCache.set(s.id, extractText(el));
        }
        for (const p of pluginsWithDoc) {
          const key = `plugin:${p.name}`;
          const el  = searchEl.querySelector<HTMLElement>(`[data-section="${cssEscape(key)}"]`);
          if (el) textCache.set(key, extractText(el));
        }
      }
      extracting = false;
      _cacheBuilding = null;
    })();
    return _cacheBuilding;
  }

  /** Match sections against the current query using the cache. */
  async function doSearch() {
    if (!searchActive) {
      matchingIds   = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }
    await ensureCache();
    const re = compiledQuery;
    if (!re) {
      matchingIds   = new Set();
      labelMatchIds = new Set();
      await applyContentHighlights();
      return;
    }

    const matches = new Set<string>();
    const labels  = new Set<string>();

    for (const s of orderedSections) {
      const labelHit = textMatches(s.label, re);
      if (labelHit) labels.add(s.id);
      if (labelHit || textMatches(textCache.get(s.id) ?? '', re)) matches.add(s.id);
    }
    for (const p of pluginsWithDoc) {
      const key = `plugin:${p.name}`;
      const labelHit = textMatches(p.name, re);
      if (labelHit) labels.add(key);
      const text = textCache.get(key) ?? '';
      if (labelHit || textMatches(text, re)) matches.add(key);
    }

    matchingIds   = matches;
    labelMatchIds = labels;

    for (const g of navGroups) {
      if (g.items.some(i => matches.has(i.id))) groupOpen[g.id] = true;
    }
    if (pluginsWithDoc.some(p => matches.has(`plugin:${p.name}`))) groupOpen.plugins = true;

    if (!matches.has(activeSection)) {
      const first = firstMatchInNavOrder();
      if (first) activeSection = first;
    }

    await applyContentHighlights();
  }

  function firstMatchInNavOrder(): string | null {
    for (const i of topItems) if (matchingIds.has(i.id)) return i.id;
    for (const g of navGroups) for (const i of g.items) if (matchingIds.has(i.id)) return i.id;
    for (const p of pluginsWithDoc) {
      const k = `plugin:${p.name}`;
      if (matchingIds.has(k)) return k;
    }
    return null;
  }

  function cssEscape(s: string): string {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : s.replace(/(["\\])/g, '\\$1');
  }

  let _searchTimer: ReturnType<typeof setTimeout> | null = null;
  function onSearchInput() {
    if (_searchTimer) clearTimeout(_searchTimer);
    _searchTimer = setTimeout(doSearch, 120);
  }

  // Re-run search when regex toggle flips.
  $effect(() => {
    searchRegex; // track
    if (searchActive) doSearch();
  });

  function clearSearch() {
    searchQuery   = '';
    matchingIds   = new Set();
    labelMatchIds = new Set();
    if (contentEl) {
      clearHighlights(contentEl, 'docs-match');
      contentMarks = [];
      currentMarkIdx = 0;
    }
  }

  // ── Content highlight injection ─────────────────────────────────────────────
  // Called from `doSearch` (debounced 120 ms) and from the section-switch
  // effect — never fires per-keystroke.
  async function applyContentHighlights() {
    await tick();
    await tick();
    if (!contentEl || !contentEl.isConnected) return;
    clearHighlights(contentEl, 'docs-match');
    contentMarks = [];
    currentMarkIdx = 0;
    const re = compiledQuery;
    if (!re || !searchActive) return;
    const marks = injectHighlights(contentEl, re, { className: 'docs-match' });
    contentMarks = marks;
    if (marks.length > 0) {
      marks[0].classList.add('current');
      marks[0].scrollIntoView({ block: 'center', behavior: 'auto' });
    }
  }

  // Re-apply highlights when the active section changes (via nav click or
  // first-match auto-select). Uses `untrack` so we don't redundantly re-run on
  // every searchQuery / searchRegex tick — those are handled by `doSearch`.
  $effect(() => {
    activeSection;
    untrack(() => { applyContentHighlights(); });
  });

  // Pre-build the text cache once when the modal mounts so the first search
  // doesn't pay the extraction cost.
  let _prebuildStarted = false;
  $effect(() => {
    if (_prebuildStarted) return;
    _prebuildStarted = true;
    // Defer slightly so the modal opens snappily; cache builds in background.
    setTimeout(() => { ensureCache(); }, 80);
  });

  // Invalidate cache when the plugin set changes. Lazy: rebuild on next search.
  let _lastPluginCount = 0;
  $effect(() => {
    const n = pluginsWithDoc.length;
    if (n !== _lastPluginCount) {
      _lastPluginCount = n;
      if (textCache.size > 0) {
        textCache.clear();
        _cacheBuilding = null;
      }
      if (searchActive) untrack(() => { doSearch(); });
    }
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

  function nextMatch() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx + 1);
    else jumpSection(+1);
  }

  function prevMatch() {
    if (contentMarks.length > 0) gotoMark(currentMarkIdx - 1);
    else jumpSection(-1);
  }

  /** Move active section to the next/previous matching one in nav order. */
  function jumpSection(dir: 1 | -1) {
    const order = navOrderIds().filter(id => matchingIds.has(id));
    if (order.length === 0) return;
    const i = order.indexOf(activeSection);
    if (i === -1) { activeSection = order[0]; return; }
    const next = (i + dir + order.length) % order.length;
    activeSection = order[next];
  }

  function navOrderIds(): string[] {
    const ids: string[] = [];
    for (const i of topItems) ids.push(i.id);
    for (const g of navGroups) for (const i of g.items) ids.push(i.id);
    for (const p of pluginsWithDoc) ids.push(`plugin:${p.name}`);
    return ids;
  }

  // F3 / Shift+F3 work anywhere within the docs modal.
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

  // ── Export state ─────────────────────────────────────────────────────────────
  let exportMode     = $state(false);
  let exportMenuOpen = $state(false);
  let exportEl       = $state<HTMLElement | null>(null);
  let exporting      = $state(false);
  let exportWrapEl   = $state<HTMLElement | null>(null);
  let pendingExport  = $state<{ content: string; defaultName: string } | null>(null);

  $effect(() => {
    if (!exportMenuOpen) return;
    function onDocClick(e: MouseEvent) {
      if (!exportWrapEl?.contains(e.target as Node)) exportMenuOpen = false;
    }
    document.addEventListener('click', onDocClick, { capture: true });
    return () => document.removeEventListener('click', onDocClick, { capture: true });
  });

  // ── Plugin docs (dynamic) ───────────────────────────────────────────────────
  let pluginsWithDoc = $state<PluginInfo[]>([]);

  function refreshPluginDocs() {
    listPluginInfo().then(list => {
      pluginsWithDoc = list.filter(p => p.doc);
    }).catch(() => {});
  }

  onMount(() => {
    refreshPluginDocs();
    return setupTauriListeners([{
      event: 'arbor://plugins-reloaded',
      handler: refreshPluginDocs,
    }]);
  });

  // ── Active section ──────────────────────────────────────────────────────────
  let activeSection = $state('getting-started');

  function selectSection(id: string) {
    activeSection = id;
    // auto-expand the containing group if collapsed
    for (const g of navGroups) {
      if (g.items.some(item => item.id === id)) {
        groupOpen[g.id] = true;
        break;
      }
    }
  }

  function isPluginSection(id: string) { return id.startsWith('plugin:'); }

  function activePluginInfo(): PluginInfo | undefined {
    if (!isPluginSection(activeSection)) return undefined;
    const name = activeSection.slice('plugin:'.length);
    return pluginsWithDoc.find(p => p.name === name);
  }

  // ── Export helpers ───────────────────────────────────────────────────────────

  /** Ordered section ids + labels, derived from the nav structure. */
  const orderedSections: { id: string; label: string; groupLabel?: string }[] = [
    ...topItems.map(i => ({ id: i.id, label: i.label })),
    ...navGroups.flatMap(g =>
      g.items.map((item, idx) => ({
        id:         item.id,
        label:      item.label,
        groupLabel: idx === 0 ? g.label : undefined,
      })),
    ),
  ];

  async function exportAs(format: 'md' | 'html') {
    exportMenuOpen = false;
    exporting      = true;

    // Render all sections into a hidden container.
    exportMode = true;
    await tick();

    if (!exportEl) { exporting = false; exportMode = false; return; }

    const sectionEls = exportEl.querySelectorAll<HTMLElement>('[data-section]');
    const elMap      = new Map<string, HTMLElement>();
    sectionEls.forEach(el => elMap.set(el.dataset.section!, el));

    const pluginEntries = pluginsWithDoc
      .filter(p => p.enabled)
      .map(p => ({ name: p.name, doc: p.doc ?? '' }));

    let content: string;
    let defaultName: string;
    let filterName: string;
    let ext: string;

    if (format === 'md') {
      const sections: SectionEntry[] = orderedSections
        .filter(s => elMap.has(s.id))
        .map(s => ({ id: s.id, label: s.label, el: elMap.get(s.id)! }));
      content     = buildReadme(sections, pluginEntries);
      defaultName = 'README.md';
      filterName  = 'Markdown';
      ext         = 'md';
    } else {
      const sections: HtmlSectionEntry[] = orderedSections
        .filter(s => elMap.has(s.id))
        .map(s => ({
          id:         s.id,
          label:      s.label,
          groupLabel: s.groupLabel,
          html:       elMap.get(s.id)!.innerHTML,
        }));
      content     = buildHtmlExport(sections, pluginEntries);
      defaultName = 'arbor-docs.html';
      filterName  = 'HTML';
      ext         = 'html';
    }

    exportMode = false;
    exporting  = false;

    pendingExport = { content, defaultName };
  }

  async function finishExport(filePath: string) {
    if (!pendingExport) return;
    const { content } = pendingExport;
    pendingExport = null;
    const fileName = filePath.split(/[\\/]/).pop() ?? filePath;
    try {
      await fsWriteTextFile(filePath, content);
      notificationsStore.add('Documentation exported', fileName, 'success');
    } catch (e) {
      notificationsStore.add('Documentation export failed', String(e), 'error');
    }
  }
</script>

<!-- Hidden render container: mounts all section components so we can read their HTML for export -->
{#if exportMode}
  <div bind:this={exportEl} class="export-offscreen">
    {#each orderedSections as s}
      {#if sectionComponents[s.id]}
        {@const SectionComp = sectionComponents[s.id]}
        <div data-section={s.id}>
          <SectionComp />
        </div>
      {/if}
    {/each}
  </div>
{/if}

<!-- Hidden render container — mounted only briefly while we extract per-section
     text into the cache. Unmounted as soon as extraction completes. -->
{#if extracting}
  <div bind:this={searchEl} class="export-offscreen" aria-hidden="true">
    {#each orderedSections as s}
      {#if sectionComponents[s.id]}
        {@const SectionComp = sectionComponents[s.id]}
        <div data-section={s.id}>
          <SectionComp />
        </div>
      {/if}
    {/each}
    {#each pluginsWithDoc as p}
      <div data-section="plugin:{p.name}">{@html p.doc ?? ''}</div>
    {/each}
  </div>
{/if}

<Modal {onClose} width="1100px" height="720px" padBody={false} ariaLabel="Documentation">
  {#snippet header()}
    <ModalHeader {onClose}>
      <BookOpen size={14} />
      <span class="modal-title">Documentation</span>
      {#snippet actions()}
        <!-- Export dropdown -->
        <div class="export-wrap" bind:this={exportWrapEl}>
          <button
            class="ps-btn"
            class:ps-btn-active={exportMenuOpen}
            onclick={() => exportMenuOpen = !exportMenuOpen}
            use:tooltip={'Export documentation'}
            aria-label="Export documentation"
            disabled={exporting}
          >
            {#if exporting}
              <Loader size={14} class="spin" />
            {:else}
              <FileDown size={14} />
            {/if}
          </button>

          {#if exportMenuOpen}
            <div class="export-menu" role="menu"
                 transition:fly={{ y: -6, duration: animStore.dFast, easing: cubicOut }}>
              <button role="menuitem" onclick={() => exportAs('md')}>
                <span class="export-ext">.md</span>
                Markdown README
              </button>
              <button role="menuitem" onclick={() => exportAs('html')}>
                <span class="export-ext">.html</span>
                Styled HTML
              </button>
            </div>
          {/if}
        </div>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="docs-body">
    <!-- Sidebar nav -->
    <nav class="docs-nav">

      <!-- Search input -->
      <div class="docs-search-wrap">
        <SearchBar
          bind:query={searchQuery}
          bind:regex={searchRegex}
          regexInvalid={regexInvalid}
          current={contentMarks.length > 0 ? currentMarkIdx + 1 : 0}
          total={contentMarks.length}
          placeholder="Search docs…"
          ariaLabel="Search documentation"
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

      <!-- Top-level items (no group) -->
      {#each topItems as item}
        {@const ItemIcon = item.icon}
        {#if !searchActive || matchingIds.has(item.id)}
          <button
            class="nav-item"
            class:active={activeSection === item.id}
            class:label-match={labelMatchIds.has(item.id)}
            onclick={() => selectSection(item.id)}
          >
            <ItemIcon size={13} />
            {#if searchActive && labelMatchIds.has(item.id)}
              <span class="nav-label">{@html highlightLabel(item.label, compiledQuery)}</span>
            {:else}
              <span class="nav-label">{item.label}</span>
            {/if}
          </button>
        {/if}
      {/each}

      <!-- Grouped items -->
      {#each navGroups as group}
        {@const GroupIcon = group.icon}
        {@const groupHits = group.items.filter(i => matchingIds.has(i.id))}
        {@const groupVisible = !searchActive || groupHits.length > 0}
        {@const expanded = searchActive ? groupHits.length > 0 : groupOpen[group.id]}

        {#if groupVisible}
          <button
            class="nav-group-header"
            onclick={() => { if (!searchActive) groupOpen[group.id] = !groupOpen[group.id]; }}
          >
            <GroupIcon size={13} />
            <span>{group.label}</span>
            {#if searchActive && groupHits.length > 0}
              <span class="nav-group-count">{groupHits.length}</span>
            {/if}
            <span class="nav-group-chevron" class:open={expanded}>
              <ChevronRight size={11} />
            </span>
          </button>

          {#if expanded}
            <div transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>
              {#each group.items as item}
                {@const ItemIcon = item.icon}
                {#if !searchActive || matchingIds.has(item.id)}
                  <button
                    class="nav-item nav-item-child"
                    class:active={activeSection === item.id}
                    class:label-match={labelMatchIds.has(item.id)}
                    onclick={() => selectSection(item.id)}
                  >
                    <ItemIcon size={12} />
                    {#if searchActive && labelMatchIds.has(item.id)}
                      <span class="nav-label">{@html highlightLabel(item.label, compiledQuery)}</span>
                    {:else}
                      <span class="nav-label">{item.label}</span>
                    {/if}
                  </button>
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      {/each}

      <!-- Plugin docs (dynamic) -->
      {#if pluginsWithDoc.length > 0}
        {@const pluginHits = pluginsWithDoc.filter(p => matchingIds.has(`plugin:${p.name}`))}
        {@const pluginsVisible = !searchActive || pluginHits.length > 0}
        {@const pluginsExpanded = searchActive ? pluginHits.length > 0 : groupOpen.plugins}

        {#if pluginsVisible}
          <button
            class="nav-group-header"
            onclick={() => { if (!searchActive) groupOpen.plugins = !groupOpen.plugins; }}
          >
            <Package size={13} />
            <span>Plugins</span>
            {#if searchActive && pluginHits.length > 0}
              <span class="nav-group-count">{pluginHits.length}</span>
            {/if}
            <span class="nav-group-chevron" class:open={pluginsExpanded}>
              <ChevronRight size={11} />
            </span>
          </button>

          {#if pluginsExpanded}
            <div transition:slide={{ duration: animStore.dPanel, easing: cubicOut }}>
              {#each pluginsWithDoc as plugin}
                {@const pid = `plugin:${plugin.name}`}
                {#if !searchActive || matchingIds.has(pid)}
                  <button
                    class="nav-item nav-item-child"
                    class:active={activeSection === pid}
                    class:label-match={labelMatchIds.has(pid)}
                    class:plugin-disabled={!plugin.enabled}
                    use:tooltip={plugin.enabled ? plugin.name : { content: plugin.name, description: 'Disabled — excluded from export' }}
                    onclick={() => selectSection(pid)}
                  >
                    {#if searchActive && labelMatchIds.has(pid)}
                      <span class="nav-label">{@html highlightLabel(plugin.name, compiledQuery)}</span>
                    {:else}
                      <span class="nav-label">{plugin.name}</span>
                    {/if}
                    {#if !plugin.enabled}
                      <span class="plugin-disabled-pill">disabled</span>
                    {/if}
                  </button>
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      {/if}
    </nav>

    <!-- Content — keyed remount on section switch so highlight injection
         never sees a half-updated DOM tree. PluginDocBlock owns the
         typography baseline (h1-h4, p, ul, code, kbd, pre, table …); the
         docs design-system utilities below (callout, feature-grid, eyebrow,
         badge, matrix, prop-list, indicator-list, hint, …) stay scoped to
         `.docs-content` because they're DocsPanel-internal authoring
         conventions used by the static section components. -->
    <div class="docs-content">
      {#key activeSection}
        <PluginDocBlock bind:innerEl={contentEl}>
          {#snippet children()}
            {#if isPluginSection(activeSection)}
              {@const plugin = activePluginInfo()}
              {#if plugin}
                {@html plugin.doc}
              {:else}
                <p style="color: var(--text-muted); margin-top: 24px;">Plugin not found.</p>
              {/if}
            {:else}
              {@const ActiveSection = sectionComponents[activeSection]}
              <ActiveSection />
            {/if}
          {/snippet}
        </PluginDocBlock>
      {/key}
    </div>
  </div>
</Modal>

<!-- Rendered AFTER the main <Modal> so the file picker stacks ON TOP of
     the docs panel (both share the same --z-modal-bg, so DOM order wins). -->
{#if pendingExport}
  <FilePickerModal
    mode="save"
    title="Export Documentation"
    initialFilename={pendingExport.defaultName}
    onConfirm={(path) => finishExport(path)}
    onCancel={() => { pendingExport = null; }}
  />
{/if}

<style>
  /* ps-btn — copied from PanelShell since DocsPanel no longer wraps its
     header buttons in PanelShell's `.ps-actions` container. */
  .ps-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    padding: 0;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .ps-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .ps-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .ps-btn-active {
    color: var(--accent);
    background: var(--accent-subtle);
  }

  /* ── Export button + dropdown ─────────────────────────────────────── */
  .export-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .export-menu {
    position: absolute;
    top: calc(100% + 5px);
    right: 0;
    min-width: 180px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md, 6px);
    box-shadow: 0 6px 20px rgba(0,0,0,0.4);
    z-index: 200;
    overflow: hidden;
    animation: fadeIn 0.1s ease;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .export-menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .export-menu button:hover { background: var(--bg-hover); color: var(--text-primary); }

  .export-ext {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--accent);
    background: rgba(77,120,204,0.12);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    min-width: 36px;
    text-align: center;
  }

  /* Hidden offscreen render container for export */
  .export-offscreen {
    position: fixed;
    left: -9999px;
    top: -9999px;
    visibility: hidden;
    pointer-events: none;
    width: 800px; /* give it a real width so components render correctly */
  }

  /* ── Body: nav + content ───────────────────────────────────────
     Mirrors the conflict modal layout: bg-elevated reveals as a 4px
     gap around floating bg-base panel cards (sidebar + content). */
  .docs-body {
    display: flex;
    flex: 1;
    height: 100%;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
  }

  /* ── Search ──────────────────────────────────────────────────── */
  .docs-search-wrap {
    margin: 8px 8px 4px;
  }

  .search-empty {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    padding: 12px 12px;
    margin: 0;
    text-align: center;
    line-height: 1.5;
    font-style: italic;
  }

  /* Match count chip on group headers */
  .nav-group-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 18px;
    height: 14px;
    padding: 0 5px;
    background: var(--accent-subtle);
    color: var(--accent);
    font-family: var(--font-ui-sans);
    font-size: 9px;
    font-weight: 700;
    border-radius: var(--radius-sm);
    margin-right: 4px;
  }

  /* Highlight inside nav labels (queryStr matched the label itself) */
  :global(.nav-label mark) {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }

  /* Highlights injected into the active section's content */
  :global(mark.docs-match) {
    background: color-mix(in srgb, var(--warning, #e8a33d) 40%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--warning, #e8a33d) 30%, transparent);
  }
  :global(mark.docs-match.current) {
    background: var(--warning, #e8a33d);
    color: var(--bg-base);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--warning, #e8a33d) 50%, transparent);
  }

  /* ── Left nav ────────────────────────────────────────────────── */
  .docs-nav {
    flex-shrink: 0;
    width: 230px;
    background: var(--bg-base);
    border-radius: 12px;
    margin-right: 4px;
    padding: 8px 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 14px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
    width: 100%;
    transition: background var(--transition-fast), color var(--transition-fast);
    border-radius: 0;
  }
  .nav-item:hover { background: rgba(255,255,255,0.04); color: var(--text-secondary); }
  .nav-item.active {
    background: rgba(77,120,204,0.14);
    color: var(--accent);
    font-weight: 500;
    border-right: 2px solid var(--accent);
  }

  /* ── Group headers ───────────────────────────────────────────── */
  .nav-group-header {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 14px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    text-align: left;
    width: 100%;
    margin-top: 6px;
    border-top: 1px solid var(--border-subtle);
    padding-top: 10px;
    transition: color var(--transition-fast);
  }
  .nav-group-header:hover { color: var(--text-secondary); }
  .nav-group-header span:first-of-type { flex: 1; }

  .nav-group-chevron {
    display: flex;
    align-items: center;
    transition: transform 150ms ease;
  }
  .nav-group-chevron.open { transform: rotate(90deg); }

  /* ── Child items (indented) ──────────────────────────────────── */
  .nav-item-child {
    padding-left: 28px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .nav-item-child:hover { color: var(--text-secondary); }
  .nav-item-child.active {
    background: rgba(77,120,204,0.10);
    color: var(--accent);
    font-weight: 500;
    border-right: 2px solid var(--accent);
  }
  .nav-item-child.plugin-disabled .nav-label {
    color: var(--text-disabled);
    font-style: italic;
    text-decoration: line-through;
    text-decoration-color: var(--text-disabled);
    opacity: 0.85;
  }
  .plugin-disabled-pill {
    margin-left: auto;
    padding: 1px 6px;
    font-size: 9px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
    background: var(--bg-hover);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
  }

  /* ── Content area ────────────────────────────────────────────── */
  .docs-content {
    flex: 1;
    overflow-y: auto;
    background: var(--bg-base);
    border-radius: 12px;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }
  .docs-content::-webkit-scrollbar { width: 5px; }
  .docs-content::-webkit-scrollbar-thumb { background: var(--border); border-radius: var(--radius-sm); }

  /* Typography baseline (h1-h4, p, ul, li, strong, kbd, code, pre, table)
     lives in `shared/internal/PluginDocBlock.svelte` so both DocsPanel and
     the Marketplace plugin-detail pane share the same reading experience.
     The design-system utilities below stay here — they're DocsPanel-internal
     authoring conventions used by the static section components. They reach
     into PluginDocBlock through `:global()` because they sit inside this
     component's `.docs-content` wrapper. */

  /* One typography exception kept here: table row hover. The widget renders
     plain rows; DocsPanel adds the hover for its denser comparison tables. */
  .docs-content :global(tbody tr:hover) { background: rgba(255,255,255,0.015); }

  /* ═══════════════════════════════════════════════════════════════════
     Doc design-system utilities
     ═══════════════════════════════════════════════════════════════════ */

  /* Lead paragraph */
  .docs-content :global(.doc-lead) {
    font-size: 13px !important;
    color: var(--text-secondary) !important;
    border-left: 3px solid var(--accent);
    padding: 8px 0 8px 14px !important;
    margin-bottom: 18px !important;
    line-height: 1.75 !important;
  }

  /* ── Feature grid ──────────────────────────────────────────────── */
  .docs-content :global(.feature-grid) {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 8px;
    margin: 12px 0;
  }
  .docs-content :global(.feature-grid.two-col) {
    grid-template-columns: repeat(2, 1fr);
  }
  .docs-content :global(.feature-card) {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .docs-content :global(.feature-card:hover) {
    border-color: var(--border);
    background: var(--bg-overlay);
  }
  .docs-content :global(.fc-title) {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .docs-content :global(.fc-title kbd) { font-size: 9px; padding: 0 3px; }
  .docs-content :global(.fc-desc) {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.6;
  }

  /* ── Step list (numbered visual steps) ────────────────────────── */
  .docs-content :global(ol.step-list) {
    padding-left: 0;
    list-style: none;
    counter-reset: step-counter;
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 12px 0;
  }
  .docs-content :global(ol.step-list > li) {
    counter-increment: step-counter;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 9px 14px 9px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.6;
  }
  .docs-content :global(ol.step-list > li::before) {
    content: counter(step-counter);
    flex-shrink: 0;
    width: 20px;
    height: 20px;
    background: var(--accent);
    color: white;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    margin-top: 1px;
  }

  /* ── Indicator legend (git graph) ─────────────────────────────── */
  .docs-content :global(.indicator-list) {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin: 10px 0;
  }
  .docs-content :global(.indicator-list > li) {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .docs-content :global(.ind) {
    width: 12px; height: 12px;
    border-radius: 50%;
    flex-shrink: 0;
    display: inline-block;
  }
  .docs-content :global(.ind-bright)  { background: var(--accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent); }
  .docs-content :global(.ind-dimmed)  { background: var(--text-muted); }
  .docs-content :global(.ind-head)    { background: transparent; box-shadow: 0 0 0 2px var(--accent); }
  .docs-content :global(.ind-merge)   { background: var(--accent); border-radius: 2px; transform: rotate(45deg); width: 10px; height: 10px; }
  .docs-content :global(.ind-wip)     { background: transparent; border: 2px dashed var(--border); }
  .docs-content :global(.ind-amber)   { background: var(--color-stash); }

  /* ── Branch label chips ────────────────────────────────────────── */
  .docs-content :global(.chip) {
    display: inline-block;
    font-family: var(--font-code);
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: 500;
    vertical-align: middle;
  }
  .docs-content :global(.chip-local)  { background: color-mix(in srgb, var(--accent) 20%, transparent);      color: var(--accent); }
  .docs-content :global(.chip-remote) { background: color-mix(in srgb, var(--color-stash) 20%, transparent); color: var(--color-stash); }
  .docs-content :global(.chip-tag)    { background: color-mix(in srgb, var(--color-tag) 20%, transparent);   color: var(--color-tag); }
  .docs-content :global(.chip-head)   { background: color-mix(in srgb, var(--success) 20%, transparent);     color: var(--success); }

  /* ── Eyebrow: accent-colored label before a heading ──────────── */
  .docs-content :global(.eyebrow) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--accent);
    background: rgba(77,120,204,0.12);
    padding: 3px 8px;
    border-radius: var(--radius-lg);
    margin: 0 0 6px;
  }

  /* ── Badges: tiny inline labels ──────────────────────────────── */
  .docs-content :global(.badge) {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 1px 6px;
    border-radius: var(--radius-lg);
    line-height: 1.6;
    vertical-align: middle;
    white-space: nowrap;
  }
  .docs-content :global(.badge-req)    { background: color-mix(in srgb, var(--error) 16%, transparent);     color: var(--error); }
  .docs-content :global(.badge-opt)    { background: color-mix(in srgb, var(--text-muted) 18%, transparent); color: var(--text-muted); }
  .docs-content :global(.badge-destr)  { background: color-mix(in srgb, var(--error) 20%, transparent);     color: var(--error); }
  .docs-content :global(.badge-async)  { background: color-mix(in srgb, var(--color-tag) 20%, transparent); color: var(--color-tag); }
  .docs-content :global(.badge-new)    { background: color-mix(in srgb, var(--success) 20%, transparent);   color: var(--success); }
  .docs-content :global(.badge-beta)   { background: color-mix(in srgb, var(--warning) 20%, transparent);   color: var(--warning); }
  .docs-content :global(.badge-accent) { background: color-mix(in srgb, var(--accent) 20%, transparent);    color: var(--accent); }

  /* ── Meta grid: compact key/value facts ──────────────────────── */
  .docs-content :global(dl.meta-grid) {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0;
    margin: 10px 0 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 4px 14px;
  }
  .docs-content :global(dl.meta-grid > dt) {
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    padding: 8px 18px 8px 0;
    border-bottom: 1px dashed var(--border-subtle);
    align-self: center;
  }
  .docs-content :global(dl.meta-grid > dd) {
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    margin: 0;
    padding: 8px 0;
    border-bottom: 1px dashed var(--border-subtle);
    line-height: 1.55;
  }
  .docs-content :global(dl.meta-grid > dt:last-of-type),
  .docs-content :global(dl.meta-grid > dd:last-of-type) { border-bottom: none; }

  /* ── Prop list: label + description vertical rows ─────────────── */
  /*
   * Implementation note: this used to be `display: grid` on `<li>` with
   * two columns, but CSS Grid promotes EVERY child (and every anonymous
   * text run) into its own grid item — so any inline `<code>` / `<strong>`
   * inside a description would create extra grid rows and visually shatter
   * the description into a list of false labels.
   *
   * The float-based layout below avoids that: the first `<strong>` /
   * `<code>` floats left with a fixed width, and the remaining inline
   * content (text + any number of `<code>` / `<em>` / `<strong>` etc.)
   * wraps naturally to the right. Multi-line descriptions stay aligned
   * because the float reserves the column.
   */
  .docs-content :global(ul.prop-list) {
    list-style: none;
    padding: 0;
    margin: 10px 0 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .docs-content :global(ul.prop-list > li) {
    padding: 8px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    line-height: 1.55;
  }
  /* Clearfix so the floated label doesn't escape its `<li>`. */
  .docs-content :global(ul.prop-list > li::after) {
    content: "";
    display: block;
    clear: both;
  }
  .docs-content :global(ul.prop-list > li > code:first-child),
  .docs-content :global(ul.prop-list > li > strong:first-child) {
    float: left;
    width: 130px;
    margin-right: 14px;
    color: var(--accent);
    font-size: var(--font-size-xs);
    font-weight: 700;
    padding-top: 1px;
  }

  /* ── Matrix table: provider/support comparison ───────────────── */
  .docs-content :global(table.matrix td.yes) {
    color: var(--success);
    font-weight: 700;
    text-align: center;
  }
  .docs-content :global(table.matrix td.no) {
    color: var(--text-disabled);
    text-align: center;
  }
  .docs-content :global(table.matrix td.partial) {
    color: var(--warning);
    font-weight: 700;
    text-align: center;
  }
  .docs-content :global(table.matrix th:not(:first-child)),
  .docs-content :global(table.matrix td:not(:first-child)) {
    text-align: center;
  }

  /* ── Hint: small inline note (lighter than callout) ──────────── */
  .docs-content :global(.hint) {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 7px 12px;
    background: rgba(77,120,204,0.06);
    border-left: 2px solid rgba(77,120,204,0.45);
    border-radius: 0 4px 4px 0;
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    margin: 8px 0;
    line-height: 1.55;
  }
  .docs-content :global(.hint::before) {
    content: 'i';
    color: var(--accent);
    font-weight: 700;
    font-style: italic;
    font-family: var(--font-code);
    flex-shrink: 0;
    width: 12px;
    height: 12px;
    background: rgba(77,120,204,0.18);
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    margin-top: 2px;
  }

  /* ── Stat row: numeric chips ─────────────────────────────────── */
  .docs-content :global(.stat-row) {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin: 10px 0 14px;
  }
  .docs-content :global(.stat) {
    flex: 1 1 120px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .docs-content :global(.stat-value) {
    font-size: 15px;
    font-weight: 700;
    color: var(--text-primary);
    font-family: var(--font-code);
  }
  .docs-content :global(.stat-label) {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
  }

  /* ── Feature card variants ───────────────────────────────────── */
  .docs-content :global(.feature-card.accent) {
    border-top: 2px solid var(--accent);
    padding-top: 10px;
  }
  .docs-content :global(.fc-eyebrow) {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--accent);
    margin-bottom: -2px;
  }

  /* ── Divider with label ──────────────────────────────────────── */
  .docs-content :global(.divider) {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 10px;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--text-muted);
  }
  .docs-content :global(.divider::before),
  .docs-content :global(.divider::after) {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--border-subtle);
  }

  /* ── Chip row: inline horizontal collection ──────────────────── */
  .docs-content :global(.chip-row) {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    vertical-align: middle;
  }

  /* ── Prism syntax token colours — bound to the theme's syntax-* tokens
     so docs code samples follow whatever palette the user picked. Falls
     back to JetBrains Darcula defaults when older theme presets are active. */
  .docs-content :global(.token.comment),
  .docs-content :global(.token.prolog),
  .docs-content :global(.token.doctype),
  .docs-content :global(.token.cdata)       { color: var(--syntax-comment, #6a9153); font-style: italic; }
  .docs-content :global(.token.string),
  .docs-content :global(.token.attr-value),
  .docs-content :global(.token.selector),
  .docs-content :global(.token.regex)       { color: var(--syntax-string, #6aab73); }
  .docs-content :global(.token.keyword),
  .docs-content :global(.token.boolean),
  .docs-content :global(.token.constant),
  .docs-content :global(.token.important)   { color: var(--syntax-keyword, #cc7832); font-weight: normal; }
  .docs-content :global(.token.number)      { color: var(--syntax-number, #6897bb); }
  .docs-content :global(.token.function),
  .docs-content :global(.token.class-name)  { color: var(--syntax-function, #ffc66d); }
  .docs-content :global(.token.property),
  .docs-content :global(.token.attr-name)   { color: var(--syntax-number, #9876aa); }
  .docs-content :global(.token.operator),
  .docs-content :global(.token.entity)      { color: var(--text-secondary); }
  .docs-content :global(.token.punctuation) { color: var(--text-muted); }
  .docs-content :global(.token.builtin)     { color: var(--syntax-type, #6897bb); }
  .docs-content :global(.token.variable)    { color: var(--text-secondary); }
  .docs-content :global(.token.parameter)   { color: var(--text-secondary); }
  .docs-content :global(.token.namespace)   { color: var(--syntax-number, #9876aa); }
  .docs-content :global(.token.tag)         { color: var(--syntax-function, #e8bf6a); }
</style>
