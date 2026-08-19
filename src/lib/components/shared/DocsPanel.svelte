<script lang="ts">
  /**
   * Arbor (Corvus) documentation — wiring on top of the shared `DocsShell`.
   * Topics live in `./docs/`; this file declares the navigation and nothing
   * else. Search, highlighting, group state and the Markdown / HTML export all
   * belong to the shell, which every product's docs panel goes through.
   *
   * The one thing Arbor's panel has that the others don't: plugin documentation,
   * which exists only at runtime and arrives as an HTML blob per plugin. It is
   * fed in as `htmlGroups` — a nav group the shell searches, renders and
   * exports like any other, with disabled plugins marked and kept out of the
   * exported file.
   */
  import { onMount } from 'svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { listPluginInfo } from '$lib/ipc/plugin';
  import type { PluginInfo } from '$lib/types/plugin';
  import DocsShell, {
    type DocsNavItem, type DocsNavGroup, type DocsHtmlGroup,
  } from './DocsShell.svelte';
  import {
    BookOpen, GitBranch, GitCommitHorizontal, GitMerge, Layers, Zap, Keyboard,
    Package, TerminalSquare, Loader, Bell, FolderGit2, Workflow,
    GitPullRequest, Search, FolderPlus, Download, Settings, TicketCheck,
    StickyNote, FolderTree, FolderOpen, History, Bug, BarChart2,
    Monitor, Database, Shield, ShieldCheck, Cloud, Tag, Share2, FolderX, Bot,
    Palette, Link2, Store, Music4,
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
  import AiToolAccess         from './docs/AiToolAccess.svelte';
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
  import FileExplorer   from './docs/FileExplorer.svelte';
  import Reflog         from './docs/Reflog.svelte';
  import Recovery       from './docs/Recovery.svelte';
  import SettingsSync   from './docs/SettingsSync.svelte';
  import MissingProjects from './docs/MissingProjects.svelte';
  import GitExecutable   from './docs/GitExecutable.svelte';
  import GitBisect      from './docs/GitBisect.svelte';
  import Statistics     from './docs/Statistics.svelte';
  import Themes         from './docs/Themes.svelte';
  import Security       from './docs/Security.svelte';
  import DeepLinks      from './docs/DeepLinks.svelte';
  import Marketplace    from './docs/Marketplace.svelte';
  import Merula          from './docs/Merula.svelte';

  let { onClose, initialSection = 'getting-started' }: {
    onClose: () => void;
    /** Topic to land on. */
    initialSection?: string;
  } = $props();

  // ── Nav structure ────────────────────────────────────────────────────────────
  const topItems: DocsNavItem[] = [
    { id: 'getting-started', label: 'Getting Started',      icon: BookOpen   },
    { id: 'init-repo',       label: 'Initialize Repository', icon: FolderPlus },
    { id: 'clone-repo',      label: 'Clone Repository',      icon: Download   },
    { id: 'workspaces',       label: 'Workspaces',         icon: Layers     },
    { id: 'linked-worktrees', label: 'Linked Worktrees',   icon: Layers     },
    { id: 'repo-browser',     label: 'Repository Browser', icon: Package    },
  ];

  const navGroups: DocsNavGroup[] = [
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
        { id: 'file-explorer',   label: 'File Explorer',      icon: FolderOpen     },
        { id: 'marketplace',     label: 'Marketplace',        icon: Store          },
        { id: 'terminal',        label: 'Terminal',           icon: TerminalSquare },
        { id: 'command-palette', label: 'Command Palette',    icon: Search         },
        { id: 'shortcuts',       label: 'Keyboard Shortcuts', icon: Keyboard       },
        { id: 'merula',           label: 'merula (Music)',      icon: Music4         },
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
        { id: 'ai-tool-access',       label: 'AI tool access',   icon: Bot       },
        { id: 'settings-sync',        label: 'Settings Sync',    icon: Cloud     },
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

  const sections = {
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
    'file-explorer':   FileExplorer,
    'reflog':          Reflog,
    'recovery':        Recovery,
    'settings-sync':   SettingsSync,
    'missing-projects': MissingProjects,
    'git-executable':  GitExecutable,
    'bisect':          GitBisect,
    'branches':        Branches,
    'submodules':      Submodules,
    'gitflow':         GitFlow,
    'terminal':        Terminal,
    'command-palette': CommandPalette,
    'shortcuts':       Shortcuts,
    'merula':           Merula,
    'jobs':            BackgroundJobs,
    'notifications':   Notifications,
    'mr':              MergeRequests,
    'issues':                IssuesDocs,
    'clone-repo':            CloneRepo,
    'repo-browser':          RepoBrowser,
    'statistics':            Statistics,
    'settings-interface':    SettingsInterface,
    'settings-performance':  SettingsPerformance,
    'settings-access':       SettingsAccess,
    'ai-tool-access':        AiToolAccess,
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

  // ── Plugin docs (runtime) ────────────────────────────────────────────────────
  let pluginsWithDoc = $state<PluginInfo[]>([]);

  function refreshPluginDocs() {
    listPluginInfo()
      .then((list) => { pluginsWithDoc = list.filter((p) => p.doc); })
      .catch(() => {});
  }

  onMount(() => {
    refreshPluginDocs();
    return setupTauriListeners([{ event: 'arbor://plugins-reloaded', handler: refreshPluginDocs }]);
  });

  // A disabled plugin keeps its page — the documentation is still true, the
  // plugin just isn't running — but stays out of the export, which describes the
  // installation as it works today.
  const htmlGroups = $derived<DocsHtmlGroup[]>([
    {
      id: 'plugins', label: 'Plugins', icon: Package,
      items: pluginsWithDoc.map((p) => ({
        id: `plugin:${p.name}`,
        label: p.name,
        html: p.doc ?? '',
        muted: !p.enabled,
        excludeFromExport: !p.enabled,
        pill: p.enabled ? undefined : 'disabled',
        tooltip: p.enabled
          ? p.name
          : { content: p.name, description: 'Disabled — excluded from export' },
      })),
    },
  ]);
</script>

<DocsShell
  {topItems}
  {navGroups}
  {htmlGroups}
  {sections}
  {onClose}
  {initialSection}
  initialOpenGroup={null}
  title="Documentation"
  headerIcon={BookOpen}
  product="Arbor"
  fileBase="arbor-docs"
  width="1100px"
  height="720px"
  prebuildSearchIndex
/>
