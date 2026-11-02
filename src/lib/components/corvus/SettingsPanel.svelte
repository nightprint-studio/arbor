<script lang="ts">
  import {
    Monitor, GitBranch, Code, Github, TicketCheck, FolderGit2, Sparkles, GitMerge, FlaskConical, Database, Layers, BarChart2, ShieldCheck, FolderX, Terminal, GitPullRequest, Settings, Workflow, ExternalLink, Link2, Boxes, Command, Store, RefreshCw,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import AppearanceSettings           from '$lib/components/shared/internal/AppearanceSettings.svelte';
  import GraphSection                 from './settings/GraphSection.svelte';
  import DiffSection                  from './settings/DiffSection.svelte';
  import GitSection                   from './settings/GitSection.svelte';
  import IssueTrackersSection         from './settings/IssueTrackersSection.svelte';
  import RepositorySection            from './settings/RepositorySection.svelte';
  import ProjectIssueTrackerSection   from './settings/ProjectIssueTrackerSection.svelte';
  import AnimationsSettings           from '$lib/components/shared/internal/AnimationsSettings.svelte';
  import KeystrokesSettings            from '$lib/components/shared/internal/KeystrokesSettings.svelte';
  import GitFlowSection               from './settings/GitFlowSection.svelte';
  import ExperimentalSection          from './settings/ExperimentalSection.svelte';
  import CacheSection                 from './settings/CacheSection.svelte';
  import IdeSection                   from './settings/IdeSection.svelte';
  import TerminalsSettings             from '$lib/components/shared/internal/TerminalsSettings.svelte';
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
  import ExplorerSection                 from './settings/ExplorerSection.svelte';
  import SyncSection                     from './settings/SyncSection.svelte';

  let { onClose, onOpenThemeEditor }: {
    onClose: () => void;
    onOpenThemeEditor: () => void;
  } = $props();

  /** The page showing (a key of `sectionComponents`). Plain string because the shell binds it. */
  let active = $state('appearance');

  const sectionComponents: Record<string, any> = {
    appearance:                  AppearanceSettings,
    animations:                  AnimationsSettings,
    keystrokes:                  KeystrokesSettings,
    graph:                       GraphSection,
    diff:                        DiffSection,
    explorer:                    ExplorerSection,
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
    terminals:                   TerminalsSettings,
    stats:                       StatsSection,
    recovery:                    RecoverySection,
    'missing-projects':          MissingProjectsSection,
    mr:                          MrSection,
    pipelines:                   PipelinesSection,
    'deep-link':                 DeepLinkSection,
    studio:                      StudioSection,
    marketplace:                 MarketplaceSection,
    sync:                        SyncSection,
  };

  const navGroups: SettingsNavGroup[] = [
    {
      label: 'Interface',
      items: [
        { id: 'appearance',  label: 'Appearance',  icon: Monitor   },
        { id: 'animations',  label: 'Animations',  icon: Sparkles  },
        { id: 'keystrokes',  label: 'Keyboard Inputs', icon: Command },
        { id: 'graph',       label: 'Graph',        icon: GitBranch },
        { id: 'diff',        label: 'Diff & Stage', icon: Code      },
        { id: 'explorer',    label: 'File Explorer', icon: FolderGit2 },
      ],
    },
    {
      label: 'Git',
      items: [
        { id: 'git-cli',           label: 'Git Executable',   icon: Terminal       },
        { id: 'gitflow',           label: 'Git Flow',         icon: GitMerge       },
        { id: 'mr',                label: 'Merge Requests',   icon: GitPullRequest },
        { id: 'recovery',          label: 'Recovery',         icon: ShieldCheck    },
        { id: 'sync',              label: 'Settings Sync',    icon: RefreshCw      },
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

  // The search (index, highlights, F3) lives in SettingsShell; the panel only says how to render a
  // page for it — see the `searchIndex` snippet below.
</script>

<Modal {onClose} width="840px" height="540px" padBody={false} ariaLabel="Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <!-- `display: contents` wrapper: carries the scope of the legacy section classes (styles below)
       without adding a box between the modal body and the shell. -->
  <div class="corvus-settings">
    <SettingsShell groups={navGroups} bind:active>
      {#snippet content()}
        {@const SectionComponent = sectionComponents[active]}
        <SectionComponent
          {...(active === 'appearance'
            ? { onOpenThemeEditor, activityBar: true, compactFileTree: true }
            : {})}
        />
      {/snippet}
      <!-- Mounted off-screen (all pages) once, on the first keystroke in the search box. -->
      {#snippet searchIndex(id: string)}
        {@const SectionComponent = sectionComponents[id]}
        <SectionComponent
          {...(id === 'appearance'
            ? { onOpenThemeEditor: () => {}, activityBar: true, compactFileTree: true }
            : {})}
        />
      {/snippet}
    </SettingsShell>
  </div>
</Modal>

<style>
  /* The frame, nav, search highlights and the common content vocabulary (.section-header, .card,
     .card-section-title, .card-row-note, .info-box, code) now come from SettingsShell. What stays
     here is the older vocabulary that Corvus's sections still write with and the shell does not
     define: rows, inputs, sliders, radios, buttons. `.corvus-settings` is a `display: contents`
     wrapper, so these reach into the shell's `.content` pane exactly as `.content` did before. */
  .corvus-settings { display: contents; }


  .corvus-settings :global(.content) :global(.card-row) {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 11px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .corvus-settings :global(.content) :global(.card-row:last-child) { border-bottom: none; }
  .corvus-settings :global(.content) :global(.card-row:hover) { background: rgba(255,255,255,0.015); }

  .corvus-settings :global(.content) :global(.card-row-column) {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 8px 14px 12px;
  }

  /* ── Row layout ─────────────────────────────────────────────────── */
  /* Title / description / control rules live in the `<FormRow>` widget
     (`fr-row`/`fr-title`/`fr-desc`/`fr-control` classes).  Only the
     `.inline-control` helper moved into `AppearanceSettings` with the section
     itself — it was the only rule here that belonged to one section. */

  /* ── Inputs ─────────────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.text-input),
  .corvus-settings :global(.content) :global(.select-input) {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    padding: 5px 8px;
    min-width: 160px;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
    outline: none;
  }
  .corvus-settings :global(.content) :global(.text-input:focus),
  .corvus-settings :global(.content) :global(.select-input:focus) {
    border-color: var(--border-focus);
    box-shadow: 0 0 0 2px rgba(77,120,204,0.15);
  }
  .corvus-settings :global(.content) :global(.text-input.narrow) { min-width: 80px; }

  .corvus-settings :global(.content) :global(.input-with-addon) {
    display: flex;
    align-items: center;
    gap: 0;
    flex: 1;
  }
  .corvus-settings :global(.content) :global(.input-with-addon .text-input) {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    border-right: none;
    flex: 1;
    min-width: 0;
  }
  .corvus-settings :global(.content) :global(.addon-btn) {
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
  .corvus-settings :global(.content) :global(.addon-btn:hover) { background: var(--bg-hover); color: var(--text-primary); }

  .corvus-settings :global(.content) :global(.template-textarea) {
    width: 100%;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.6;
    padding: 8px 10px;
    resize: vertical;
    transition: border-color var(--transition-fast);
    box-sizing: border-box;
  }
  .corvus-settings :global(.content) :global(.template-textarea:focus) { outline: none; border-color: var(--border-focus); }
  .corvus-settings :global(.content) :global(.template-textarea::placeholder) { color: var(--text-disabled); }

  /* ── Slider ─────────────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.slider) {
    flex: 1;
    accent-color: var(--accent);
    cursor: pointer;
    height: 4px;
  }
  .corvus-settings :global(.content) :global(.value-chip) {
    font-size: var(--font-size-xs);
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
  .corvus-settings :global(.content) :global(.radio-group) { display: flex; gap: 6px; }
  .corvus-settings :global(.content) :global(.radio-option) {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    background: var(--bg-input);
    transition: all var(--transition-fast);
    user-select: none;
  }
  .corvus-settings :global(.content) :global(.radio-option input) { display: none; }
  .corvus-settings :global(.content) :global(.radio-option:hover) { border-color: var(--border-focus); color: var(--text-primary); }
  .corvus-settings :global(.content) :global(.radio-option.selected) {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 500;
  }

  /* ── Buttons ────────────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.btn-primary) {
    padding: 6px 18px;
    background: var(--accent);
    color: var(--text-on-accent);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    transition: filter var(--transition-fast), opacity var(--transition-fast);
  }
  .corvus-settings :global(.content) :global(.btn-primary:hover:not(:disabled)) { filter: brightness(1.12); }
  .corvus-settings :global(.content) :global(.btn-primary:disabled) { opacity: 0.45; cursor: not-allowed; }

  /* `.btn-ghost` and `.btn-ghost-danger` used to live here, as scoped globals
     that reached into whatever this panel rendered — including SHARED widgets
     (the provider cards' Disconnect). Two problems: those widgets were styled
     correctly only while mounted inside this panel, and the rules also hijacked
     `<Button variant="ghost">`, overriding the design system's own borderless
     ghost with a bordered one whenever a Button happened to sit in Settings.
     Both looks are now `Button` variants — `ghost` and `outline` — so they
     travel with the widget instead of with its container. */

  .corvus-settings :global(.content) :global(.btn-ghost-sm) {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 11px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: all var(--transition-fast);
  }
  .corvus-settings :global(.content) :global(.btn-ghost-sm:hover) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border-focus);
  }

  /* ── Icon buttons ───────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.icon-btn) {
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
  .corvus-settings :global(.content) :global(.icon-btn:hover) { background: var(--bg-hover); color: var(--text-primary); }
  .corvus-settings :global(.content) :global(.icon-btn.danger:hover) { background: var(--error-subtle); color: var(--error); }

  /* ── Empty state ────────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.empty-state) {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 36px 20px;
    color: var(--text-disabled);
    font-size: var(--font-size-sm);
    background: var(--bg-elevated);
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
    text-align: center;
  }
  .corvus-settings :global(.content) :global(.empty-hint) { font-size: var(--font-size-xs); color: var(--text-muted); max-width: 280px; line-height: 1.5; }

  .corvus-settings :global(.content) :global(.loading-dots) { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.4; } }

  /* ── Form ───────────────────────────────────────────────────────── */
  .corvus-settings :global(.content) :global(.form-error) {
    font-size: var(--font-size-xs);
    color: var(--error);
    margin: 0;
    padding: 0 2px;
  }
  .corvus-settings :global(.content) :global(.form-actions) {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .corvus-settings :global(.content) :global(.saved-label) {
    font-size: var(--font-size-xs);
    color: var(--success, #6aab73);
  }
</style>
