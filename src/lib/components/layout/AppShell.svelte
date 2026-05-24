<script lang="ts">
  import { onMount } from 'svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { coalesceLatestByKey } from '$lib/utils/coalesce';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import TitleBar from './TitleBar.svelte';
  import TabBar from './TabBar.svelte';
  import StatusBar from './StatusBar.svelte';
  import ActivityBarLeft from './ActivityBarLeft.svelte';
  import ActivityBarRight from './ActivityBarRight.svelte';
  import PluginSidebarPanel from '../plugins/PluginSidebarPanel.svelte';
  import PluginTreeSidebar from '../plugins/PluginTreeSidebar.svelte';
  import ResizablePanel from './ResizablePanel.svelte';
  import Sidebar from '../sidebar/Sidebar.svelte';
  import CommitGraph from '../graph/CommitGraph.svelte';
  import CommitDetailPanel from '../graph/CommitDetailPanel.svelte';
  import StageArea from '../stage/StageArea.svelte';
  import WelcomeScreen from '../shared/WelcomeScreen.svelte';
  import BootSplash    from '../shared/BootSplash.svelte';
  import MissingRepoState from '../shared/MissingRepoState.svelte';
  // SettingsPanel / DocsPanel / AboutModal / *StudioModal are loaded lazily
  // through <Lazy /> below — see the gate-keyed instances near the modal
  // section. The dev warmup in onMount pre-fires their loaders so local
  // builds don't pay a first-open delay.
  import Lazy from '../shared/Lazy.svelte';
  // Re-enable together with the dev-warmup block below when needed.
  // import { dev } from '$app/environment';
  import PluginPanel from '../plugins/PluginPanel.svelte';
  import GitFlowPanel from '../sidebar/GitFlowPanel.svelte';
  // PluginFormModal + ContributableModal are loaded lazily through <Lazy />
  // below so the whole FormNode* sub-renderer tree + PluginPipelineEditor
  // stay out of the initial JS heap. Both modals share the same chunk
  // (FormNodeRenderer.svelte) — Rollup deduplicates automatically.
  import { containerStore }          from '$lib/stores/container.svelte';
  import GitBlameModal from '../shared/GitBlameModal.svelte';
  import { hasOpenModal } from '../shared/Modal.svelte';
  import ToastItem from '../shared/Toast.svelte';
  import TerminalPanel from '../terminal/TerminalPanel.svelte';
  import JobsOverlay from '../jobs/JobsOverlay.svelte';
  import JobOutputPanel from '../jobs/JobOutputPanel.svelte';
  import PluginLogsPanel from '../plugins/PluginLogsPanel.svelte';
  import { pluginLogsStore } from '$lib/stores/pluginLogs.svelte';
  import PipelinesPanel from '../pipeline/PipelinesPanel.svelte';
  import PipelineRunDetailModal from '../pipeline/PipelineRunDetailModal.svelte';
  import NotificationsOverlay from '../shared/NotificationsOverlay.svelte';
  import KeystrokesOverlay   from '../shared/KeystrokesOverlay.svelte';
  import { keystrokesStore }  from '$lib/stores/keystrokes.svelte';
  import NotificationItem    from '../shared/NotificationItem.svelte';
  import OperationsOverlay   from '../shared/OperationsOverlay.svelte';
  import { setupOperationBridge } from '$lib/utils/operations-bridge';
  import RecentReposModal from '../shared/RecentReposModal.svelte';
  import DepsExplorerModal from '../shared/DepsExplorerModal.svelte';
  import { depsExplorerStore } from '$lib/stores/depsExplorer.svelte';
  import CommandPalette from '../shared/CommandPalette.svelte';
  import Tooltip from '../shared/Tooltip.svelte';
  import ConflictResolutionModal from '../stage/conflict/ConflictResolutionModal.svelte';
  import CheckoutConflictModal from '../shared/CheckoutConflictModal.svelte';
  import InitRepoModal from '../shared/InitRepoModal.svelte';
  import CloneRepoModal from '../shared/CloneRepoModal.svelte';
  import GitSetupModal from '../shared/GitSetupModal.svelte';
  import OnboardingModal from '../shared/OnboardingModal.svelte';
  import { onboardingStore } from '$lib/stores/onboarding.svelte';
  import { gitCliStore } from '$lib/stores/gitCli.svelte';
  import RepoBrowserModal from '../shared/RepoBrowserModal.svelte';
  // Cloud bulk transfers surface via the standard JobRegistry → JobsOverlay
  // flow: every download_many / sync registers a JobInfo with category
  // "Cloud Storage". The bespoke aggregate-progress floater that used to
  // live here was duplicative — the user already sees the running job in
  // JobsOverlay and gets per-file detail in JobOutputPanel.
  import CloudChunkOrderModal from '../shared/CloudChunkOrderModal.svelte';
  import { repoBrowserStore } from '$lib/stores/repoBrowser.svelte';
  import FilePickerModal from '../shared/FilePickerModal.svelte';
  import BisectBanner from '../shared/BisectBanner.svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import StatsPanel from '../sidebar/StatsPanel.svelte';
  import SecurityPanel from '../security/SecurityPanel.svelte';
  import StudioPanel from '../sidebar/StudioPanel.svelte';
  import SecurityQuickOverlay from '../security/SecurityQuickOverlay.svelte';
  import StatsOverlay from '../stats/StatsOverlay.svelte';
  import { statsStore } from '$lib/stores/stats.svelte';
  import { securityStore } from '$lib/stores/security.svelte';
  import type { SecuritySummary } from '$lib/types/security';
  import ThemeEditorModal from '../theme/ThemeEditorModal.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { brandingStore } from '$lib/stores/branding.svelte';
  import { jsonStudioStore } from '$lib/stores/json-studio.svelte';
  import { ronStudioStore } from '$lib/stores/ron-studio.svelte';
  import { tomlStudioStore } from '$lib/stores/toml-studio.svelte';
  import { yamlStudioStore } from '$lib/stores/yaml-studio.svelte';
  import { propertiesStudioStore } from '$lib/stores/properties-studio.svelte';
  import { activityBarConfigStore } from '$lib/stores/activityBarConfig.svelte';
  import MrSidebar from '../mr/MrSidebar.svelte';
  import MrModal from '../mr/MrModal.svelte';
  import FileTreePanel from '../sidebar/FileTreePanel.svelte';
  import ReflogPanel from '../sidebar/ReflogPanel.svelte';
  import CreateMrModal from '../mr/CreateMrModal.svelte';
  import AddWorktreeModal from '../sidebar/AddWorktreeModal.svelte';
  import WorktreeInfoModal from '../sidebar/WorktreeInfoModal.svelte';
  import type { WorktreeInfo } from '$lib/types/git';
  import { switchToWorktree } from '$lib/utils/worktree-switch';
  import { openInIde } from '$lib/ipc/worktree';
  import CiPipelineDetailModal from '../pipeline/CiPipelineDetailModal.svelte';
  import DeepLinkConfirmModal from '../shared/DeepLinkConfirmModal.svelte';
  import DeepLinkActionConfirmModal from '../shared/DeepLinkActionConfirmModal.svelte';
  import DeepLinkDisabledModal from '../shared/DeepLinkDisabledModal.svelte';
  import DeepLinkLoadingModal from '../shared/DeepLinkLoadingModal.svelte';
  import { deepLinkDispatcher } from '$lib/utils/deep-link-dispatcher.svelte';
  import { deepLinkReady } from '$lib/ipc/deep-link';
  import { getMrDetail } from '$lib/ipc/mr';
  import type { CiRun } from '$lib/types/pipeline';
  import { listen } from '@tauri-apps/api/event';
  import IssuesSidebar from '../issues/IssuesSidebar.svelte';
  import BranchRenameModal from '../sidebar/BranchRenameModal.svelte';
  import DeleteTagModal from '../sidebar/DeleteTagModal.svelte';
  import { executeTagDelete as runTagDelete } from '$lib/utils/tag-delete';
  import type { BranchInfo } from '$lib/types/git';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  // Custom transition: collapses/expands width (proper IDE sidebar behaviour).
  function sidebarSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const w = node.getBoundingClientRect().width;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) => `width: ${t * w}px; min-width: 0; overflow: hidden;`,
    };
  }

  // Custom transition: collapses/expands height (IDE bottom panel behaviour).
  function bottomSlide(node: HTMLElement, { duration = 200 }: { duration?: number } = {}) {
    const h = node.getBoundingClientRect().height;
    return {
      duration,
      easing: cubicOut,
      css: (t: number) => `height: ${t * h}px; min-height: 0; overflow: hidden;`,
    };
  }
  import { animStore } from '$lib/stores/animations.svelte';
  import { tabsStore, setTabsPersistHook } from '$lib/stores/tabs.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import WorkspaceManagementModal from '../workspace/WorkspaceManagementModal.svelte';
  import WorktreeLinkManagerModal from '../linked-worktrees/WorktreeLinkManagerModal.svelte';
  import AddToWorktreeLinkModal from '../linked-worktrees/AddToWorktreeLinkModal.svelte';
  import WorktreeLinkSyncSummary from '../linked-worktrees/WorktreeLinkSyncSummary.svelte';
  import CreateWorkspaceModal from '../workspace/CreateWorkspaceModal.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import { jobsStore } from '$lib/stores/jobs.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { commitConfigStore } from '$lib/stores/commit_config.svelte';
  import { terminalStore } from '$lib/stores/terminal.svelte';
  import { pipelinesStore } from '$lib/stores/pipelines.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { openRepo, checkIsGitRepo, initRepo } from '$lib/ipc/graph';
  import {
    validateRepoPath, validateRepoPaths, reportRepoMissing,
    getMissingProjectsConfig, removeRecentRepo,
  } from '$lib/ipc/missing';
  import type { MissingProjectsConfig } from '$lib/ipc/missing';
  import type { InitRepoOptions } from '$lib/types/git';
  import { terminalCreate } from '$lib/ipc/terminal';
  import { keybindingsStore } from '$lib/stores/keybindings.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { mrStore } from '$lib/stores/mr.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { worktreeStore } from '$lib/stores/worktree.svelte';
  import { startIdeDetection } from '$lib/ipc/worktree';
  import { startShellDetection } from '$lib/ipc/terminal';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { linearGetIssue, jiraGetIssue } from '$lib/ipc/issues';
  import { matchesBinding } from '$lib/utils/keybindings';
  import { firePluginAction, listPluginInfo } from '$lib/ipc/plugin';
  import type { PluginFormConfig } from '$lib/types/plugin';
  import type { MergeRequest } from '$lib/types/mr';

  // Font scale + theme-font opt-in live in `appearanceStore` (persisted in
  // config.toml). The store applies the default `--font-scale` synchronously
  // on import; `appearanceStore.loadConfig()` (called in onMount below)
  // overwrites it with the on-disk value once the backend answers.

  // ── Missing-projects config (tombstone + locate behaviour) ────────────────
  let missingConfig = $state<MissingProjectsConfig>({
    auto_prune_recents:    false,
    confirm_before_remove: true,
    revalidate_on_focus:   true,
  });
  onMount(async () => {
    try { missingConfig = await getMissingProjectsConfig(); }
    catch { /* keep defaults on first launch */ }
  });

  // ── Git executable detection ─────────────────────────────────────────────
  // Resolve which `git` binary Arbor should use BEFORE the rest of the boot
  // touches CLI shell-outs.  When detection fails, GitSetupModal renders as
  // a blocking overlay until the user picks a path / downloads PortableGit.
  onMount(async () => { await gitCliStore.init(); });

  // ── Dev warmup (disabled) ────────────────────────────────────────────────
  // Pre-fires the loaders for the lazy panels/modals so first-open feels
  // instant during local development. `dev` is a static constant from
  // SvelteKit — in `vite build` it is `false` and Rollup drops the entire
  // block, preserving the release-build RAM win.
  //
  // Currently commented out so dev mirrors release behaviour, which makes
  // it easier to profile the actual lazy-load latency. Re-enable when the
  // first-open delay starts hurting daily development.
  // onMount(() => {
  //   if (!dev) return;
  //   void import('./SettingsPanel.svelte');
  //   void import('../shared/DocsPanel.svelte');
  //   void import('../shared/AboutModal.svelte');
  //   void import('$lib/components/shared/JsonStudioModal.svelte');
  //   void import('$lib/components/shared/RonStudioModal.svelte');
  //   void import('$lib/components/shared/TomlStudioModal.svelte');
  //   void import('$lib/components/shared/YamlStudioModal.svelte');
  //   void import('$lib/components/shared/PropertiesStudioModal.svelte');
  //   void import('../plugins/PluginFormModal.svelte');
  //   void import('../plugins/ContributableModal.svelte');
  // });

  // ── Deep-link router (`arbor://…`) ───────────────────────────────────────
  // The backend buffers any URLs that arrived during cold-start until we
  // call `deepLinkReady` here, then forwards every subsequent open via the
  // `arbor://deep-link` event.  The dispatcher drives:
  //   * tab activation / workspace switch / cross-WS open
  //   * clone-confirm modal when the target repo isn't local
  //   * follow-up actions (jump to commit, MR detail, CI detail, …)
  // Each follow-up that needs a modal is wired through callbacks below.
  onMount(() => {
    let unlistenDl: (() => void) | null = null;

    deepLinkDispatcher.wire({
      openWorktreeModal: (tabId, branch) => {
        if (tabsStore.activeTabId !== tabId) tabsStore.setActive(tabId);
        dlWorktreeState = { tabId, branch };
      },
      openMrDetail: async (tabId, number) => {
        if (tabsStore.activeTabId !== tabId) tabsStore.setActive(tabId);
        // Show the loading placeholder synchronously — `getMrDetail` is a
        // round-trip to the remote provider and can take several seconds.
        // Without this the user clicks the link and stares at an unchanged
        // screen until the fetch lands.
        dlLoading = { title: `Opening merge request !${number}`, status: 'loading' };
        try {
          // Short-circuit when MR/PRs are disabled on the remote so the user
          // sees the real reason instead of a generic 404 from get_mr_detail.
          // The probe is cached per tab so this is usually free.
          const feature = await cacheStore.loadMrFeature(tabId);
          if (!feature.enabled) {
            dlLoading = {
              title:   `Merge request !${number}`,
              status:  'error',
              message: feature.reason ??
                'Merge requests are disabled on this repository.',
            };
            return;
          }
          const detail = await getMrDetail(tabId, number);
          dlLoading = null;
          openMrDetail(detail.mr);
        } catch (e) {
          dlLoading = {
            title:   `Merge request !${number}`,
            status:  'error',
            message: isNotFound(e)
              ? `MR !${number} doesn't exist or you don't have access to it on this remote.`
              : `Failed to load MR !${number}: ${e}`,
          };
        }
      },
      openCiDetail: async (tabId, runId) => {
        if (tabsStore.activeTabId !== tabId) tabsStore.setActive(tabId);
        dlLoading = { title: `Opening CI run ${runId}`, status: 'loading' };
        try {
          // Make sure runs are populated for this tab; the panel does this
          // lazily but the deep-link can target any tab independently.
          if (!pipelinesStore.ciRuns.length || tabsStore.activeTabId !== tabId) {
            await pipelinesStore.loadCi(tabId);
          }
          const run = pipelinesStore.ciRuns.find(r => String(r.id) === String(runId));
          if (!run) {
            dlLoading = {
              title:   `CI run ${runId}`,
              status:  'error',
              message: `CI run ${runId} wasn't found in the latest pipeline list for this repository.`,
            };
            return;
          }
          dlLoading = null;
          dlCiRun   = run;
          dlCiTabId = tabId;
        } catch (e) {
          dlLoading = {
            title:   `CI run ${runId}`,
            status:  'error',
            message: isNotFound(e)
              ? `CI run ${runId} doesn't exist or you don't have access to it.`
              : `Failed to load CI run ${runId}: ${e}`,
          };
        }
      },
    });

    // Order matters on cold-start: `listen()` is async (it round-trips to
    // register the listener on the backend), so we MUST await it before
    // telling the backend to flush its buffered URLs — otherwise the
    // flushed events race the registration and get dropped on the floor,
    // which is exactly the cold-start bug ("app opens but the rest of the
    // link doesn't happen"). Wrap in an IIFE so we can still return the
    // sync cleanup that Svelte's onMount expects.
    (async () => {
      unlistenDl = await listen<string>('arbor://deep-link', ev => {
        void deepLinkDispatcher.dispatch(ev.payload);
      });
      try { await deepLinkReady(); } catch { /* IPC bridge broken — nothing else works either */ }
    })();

    return () => { unlistenDl?.(); };
  });

  /** Re-classify every tombstoned tab.  Called when the window regains
   *  focus (config-gated) and after the user clicks Retry on a tombstone
   *  panel.  A tab promoted to OK is reopened in-place via the bridge. */
  async function revalidateTombstonedTabs(): Promise<void> {
    const candidates = tabsStore.tabs.filter(t => t.tombstone);
    if (candidates.length === 0) return;
    let validations;
    try { validations = await validateRepoPaths(candidates.map(t => t.path)); }
    catch { return; }
    for (let i = 0; i < candidates.length; i++) {
      const tab = candidates[i];
      const v = validations[i];
      if (!v) continue;
      if (v.status === 'ok') {
        // Path came back to life — open the repo and shed the tombstone.
        try {
          const info = await openRepo(tab.path, tab.id);
          tabsStore.updateTab(tab.id, {
            info, name: info.name, currentBranch: info.current_branch ?? null,
            status: null, tombstone: null,
          });
        } catch { /* still bad; leave as tombstone */ }
      } else {
        tabsStore.setTombstone(tab.id, {
          reason: v.status, message: v.message, checkedAt: Date.now(),
        });
      }
    }
  }

  // ── Init repo modal state ──────────────────────────────────────────────────
  let initModalOpen = $state(false);
  let initModalPath = $state('');

  // ── Clone repo modal state ─────────────────────────────────────────────────
  let cloneModalOpen = $state(false);

  // ── Open repo file picker state ────────────────────────────────────────────
  let openPickerOpen = $state(false);
  // True when the folder picker should route to the Init flow on confirm
  // (regardless of whether the picked folder is already a repo — see below).
  let openPickerForInit = $state(false);

  // ── GitSetupModal bouncer ──────────────────────────────────────────────────
  // The modal pops up when detection reports `missing`. Users with git already
  // installed (e.g. PATH not yet picked up after a fresh install, or a custom
  // location they want to Browse to) need an escape hatch — making the X
  // button work doesn't help if `phase==='missing'` keeps re-rendering it.
  // So we track a dismissed flag here and reset it when phase transitions
  // back to `ready` so future detection failures still surface the modal.
  let gitBouncerDismissed = $state(false);
  $effect(() => {
    if (gitCliStore.phase === 'ready') gitBouncerDismissed = false;
  });

  // ── Plugin-requested file picker state ─────────────────────────────────────
  // Populated when any plugin calls `arbor.ui.pick_file(opts)`. On confirm we
  // call firePluginAction with the chosen path; on cancel we fire the action
  // with an empty path so the plugin can differentiate vs. a successful pick.
  type PluginPickFile = {
    plugin_name: string;
    mode?: 'file' | 'folder' | 'save';
    title?: string;
    extensions?: string[];
    initial_path?: string;
    action: string;
    extra?: Record<string, unknown>;
  };
  let pluginPickFile = $state<PluginPickFile | null>(null);

  function handleOpenRepo(path?: string) {
    if (path) {
      _openRepoPath(path);
    } else {
      openPickerForInit = false;
      openPickerOpen = true;
    }
  }

  function startInitRepoFlow() {
    openPickerForInit = true;
    openPickerOpen = true;
  }

  async function handlePickerConfirm(path: string) {
    openPickerOpen = false;
    if (openPickerForInit) {
      openPickerForInit = false;
      // Init mode: if the folder is already a git repo just open it,
      // otherwise jump straight into the InitRepoModal.
      let isGit = false;
      try { isGit = await checkIsGitRepo(path); } catch { /* assume not a repo */ }
      if (isGit) {
        uiStore.showToast('Folder is already a git repository — opening it instead', 'info');
        await _openRepoPath(path);
      } else {
        initModalPath = path;
        initModalOpen = true;
      }
      return;
    }
    await _openRepoPath(path);
  }

  async function _openRepoPath(selected: string) {
    // If the repo is already open, just switch to its tab.  An existing
    // tombstone tab gets reactivated AND the user is shown the locate UI
    // immediately (vs. silently re-failing to open).
    const existing = tabsStore.tabs.find(t => t.path === selected);
    if (existing) { tabsStore.setActive(existing.id); return; }

    // Pre-flight: classify the path before touching git2.  Distinguishes
    // missing/unreachable from "exists but not a repo" so we can offer the
    // right UX (init prompt for not-a-repo, removed-from-disk toast for
    // freshly-deleted recents).
    let validation;
    try { validation = await validateRepoPath(selected); }
    catch { validation = null; }

    if (validation && (validation.status === 'missing' || validation.status === 'unreachable')) {
      uiStore.showToast(
        validation.status === 'missing'
          ? `Folder no longer exists: ${selected}`
          : `Drive or share unavailable: ${selected}`,
        'error',
      );
      // Drop dead recents so the WelcomeScreen list stops showing them.
      if (missingConfig.auto_prune_recents) {
        try { await removeRecentRepo(selected); } catch { /* non-critical */ }
        await uiStore.loadRecentRepos();
      }
      return;
    }

    // Check if the selected folder is already a git repository.
    let isGit = true;
    try { isGit = await checkIsGitRepo(selected as string); } catch { /* assume true on error */ }

    if (!isGit) {
      // Not a git repo — prompt the user to initialise one.
      initModalPath = selected as string;
      initModalOpen = true;
      return;
    }

    try {
      // Register in the central repo registry first — the returned id is
      // used as the tab id so tab identity is stable across workspace
      // switches.  This also auto-adds the repo to the active workspace.
      const repoId = await workspacesStore.ensureRepoRegistered(selected as string);
      const info   = await openRepo(selected as string, repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(selected as string);
      uiStore.showToast(`Opened ${info.name}`, 'success');
    } catch (err) {
      uiStore.showToast(`Failed to open repo: ${err}`, 'error');
    }
  }

  async function handleInitRepo(opts: InitRepoOptions) {
    initModalOpen = false;
    try {
      const repoId = await workspacesStore.ensureRepoRegistered(initModalPath);
      const result = await initRepo(initModalPath, repoId, opts);
      tabsStore.addTab(result.info);
      uiStore.addRecentRepo(initModalPath);

      const name = result.info.name;
      const hasRemote = !!result.remote_url;
      const hasCommit = opts.initial_commit;

      // Toast for the init itself — mention the push result inline when attempted.
      if (result.pushed) {
        uiStore.showToast(`Initialized ${name} and pushed to origin`, 'success');
      } else if (opts.push_initial && result.push_error) {
        uiStore.showToast(`Initialized ${name} — push failed`, 'warning');
      } else {
        uiStore.showToast(`Initialized ${name}`, 'success');
      }

      // Persistent notifications so the user doesn't lose the "still-local" status
      // after the toast fades. Only fire when it's actually actionable.
      if (opts.push_initial && result.push_error) {
        notificationsStore.add(
          `Initial push failed for ${name}`,
          `Repository initialized locally. Push to origin failed: ${result.push_error}. Fix credentials in Settings → Authentication, then push manually.`,
          'warning',
        );
      } else if (hasRemote && hasCommit && !result.pushed) {
        // Remote configured, initial commit created, but push was not requested.
        notificationsStore.add(
          `${name} is local only`,
          `The initial commit exists locally but has not been pushed to origin. Run Push to publish it.`,
          'info',
        );
      }
    } catch (err) {
      uiStore.showToast(`Failed to initialize repository: ${err}`, 'error');
    }
  }

  // Initialise theme store — loads active theme from config and applies CSS vars.
  onMount(() => { themeStore.init(); });

  // Hydrate plugin-applied branding (logo) from the backend snapshot, then
  // keep it in sync via arbor://branding-changed events. Done up-front so
  // the title bar never flashes the default Arbor mark when a plugin
  // installed an override during on_plugin_load.
  onMount(() => { void brandingStore.init(); });

  // Load persisted diff config (full-file toggle, virt threshold) from
  // ~/.config/arbor/config.toml. Defaults are applied immediately so first
  // paint isn't blocked; this just refreshes them once disk values are read.
  onMount(() => { void diffStore.loadConfig(); });

  // Same flow for appearance (window controls, font scale, theme-font opt-in),
  // animations (enabled + speed), and commit (global template fallback):
  // defaults render now, disk values overwrite once read.
  onMount(() => { void appearanceStore.loadConfig(); });
  onMount(() => { void animStore.loadConfig(); });
  onMount(() => { void commitConfigStore.loadConfig(); });

  // ── Onboarding tour ──────────────────────────────────────────────────────
  // Load the persisted onboarding state up front. The auto-open trigger is
  // deferred to the effect below so we can wait for git detection to settle
  // — opening on top of the GitSetupModal bouncer would just stack two
  // dialogs in the user's face.
  onMount(() => { void onboardingStore.loadConfig(); });

  // Auto-open once when: (a) config has loaded, (b) git detection has reached
  // a non-blocking phase, (c) the user hasn't been through the current
  // onboarding schema version yet, (d) the bouncer isn't on screen. Wrapped
  // in a fired-once guard so reloading the store later (e.g. via "Reset
  // onboarding" in Settings) doesn't re-pop it mid-session — the store's
  // `show()` is the explicit re-entry path.
  let _onboardingAutoFired = $state(false);
  $effect(() => {
    if (_onboardingAutoFired)               return;
    if (!onboardingStore.loaded)            return;
    if (!onboardingStore.shouldAutoOpen())  return;
    // Defer until git detection has settled.  `ready` is the happy path;
    // `failed` still lets the user proceed without git (e.g. they'll
    // pick a path later) so the tour is still useful.
    const phase = gitCliStore.phase;
    if (phase !== 'ready' && phase !== 'failed') return;
    _onboardingAutoFired = true;
    onboardingStore.show();
  });

  // Manual re-entry from Command Palette / Docs / Settings.
  $effect(() => {
    function open() { onboardingStore.show(); }
    window.addEventListener('arbor:open-onboarding', open);
    return () => window.removeEventListener('arbor:open-onboarding', open);
  });

  // Initialise the data cache and run one-time IDE detection at startup.
  onMount(async () => {
    cacheStore.init(() => tabsStore.activeTabId);
    // Load IDE config from disk (instant).
    worktreeStore.loadIdeConfig();
    // Register the listener before firing detection so no event is missed.
    await worktreeStore.setupDetectionListener();
    // Kick off IDE detection as a non-cancellable background job.
    // Results arrive via the arbor://ide-detection-done event.
    startIdeDetection().catch(() => {/* ignore if backend unavailable during HMR */});

    // Same flow for the integrated terminal: load catalogue + config from
    // disk, register the listener, then fire detection.
    terminalStore.loadCatalogue();
    terminalStore.loadConfig();
    await terminalStore.setupDetectionListener();
    startShellDetection().catch(() => {/* HMR */});
  });

  // Keep cacheStore in sync with the active tab so the scheduler and
  // StatusBar's "last refreshed" badge know which tab is current.
  $effect(() => {
    cacheStore.setActiveTabId(tabsStore.activeTabId);
  });

  // Restore previously open tabs on startup via the workspace subsystem.
  // The workspace store knows which repos belong to the active workspace +
  // which ones were open in its snapshot, and drives opens/closes through
  // the bridge we wire below.  Single-shot via onMount (never $effect).
  onMount(async () => {
    try {
      setTabsPersistHook(() => { void workspacesStore.persistSnapshotNow(); });

      // Recent repos for the WelcomeScreen CTA and other consumers.
      await uiStore.loadRecentRepos();

      // Wire a bridge that lets the workspace store open/close/activate
      // actual tabs.  The store runs load() internally and then replays
      // the active workspace's snapshot.
      await workspacesStore.bootstrap({
        openRepo: async (path, tabId) => {
          // Pre-flight classify the path so a missing/unreachable folder
          // surfaces as a tombstone tab instead of being silently skipped.
          // Avoids the "tab vanished" UX when a project moves under us.
          let validation;
          try { validation = await validateRepoPath(path); }
          catch { validation = null; }

          if (validation && validation.status !== 'ok') {
            const fallbackName = path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? 'repository';
            const ws = workspacesStore.registryById.get(tabId);
            tabsStore.addTombstoneTab({
              id:      tabId,
              path,
              name:    ws?.display_name ?? fallbackName,
              reason:  validation.status,
              message: validation.message,
              silent:  true,
            });
            void reportRepoMissing(tabId, path, validation.status).catch(() => {});
            return;
          }

          try {
            const info = await openRepo(path, tabId);
            tabsStore.addTabSilent(info);
            uiStore.addRecentRepo(path);
          } catch (err) {
            // Open failed for a non-classification reason (e.g. libgit2
            // permissions / corrupt repo).  Treat as not_a_repo so the user
            // still gets the tombstone UI rather than nothing.
            const fallbackName = path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? 'repository';
            const ws = workspacesStore.registryById.get(tabId);
            tabsStore.addTombstoneTab({
              id:      tabId,
              path,
              name:    ws?.display_name ?? fallbackName,
              reason:  'not_a_repo',
              message: `Could not open: ${err}`,
              silent:  true,
            });
            void reportRepoMissing(tabId, path, 'not_a_repo').catch(() => {});
          }
        },
        closeTab: async (tabId) => {
          try { await import('$lib/ipc/graph').then(m => m.closeRepo(tabId)); } catch { /* ignore */ }
          // Silent removal — workspace swap clears activeTabId separately
          // so we don't want a cascade that reactivates adjacent tabs
          // mid-loop (that fired "Repository not open" errors).
          tabsStore.removeTabSilent(tabId);
        },
        setActiveTab:   (tabId) => { tabsStore.setActive(tabId); },
        clearActiveTab: ()       => { tabsStore.clearActive(); },
        currentOpenTabIds:  () => tabsStore.tabs.map(t => t.id),
        currentActiveTabId: () => tabsStore.activeTabId,
        currentTabMeta: () =>
          tabsStore.tabs
            // Only persist tabs whose state diverges from the default
            // (path-derived name, no worktree icon).  Keeps the snapshot
            // file small and forward-compatible.
            .filter(t => t.isLinkedWorktree || (t.info && t.name !== t.info.name))
            .map(t => ({
              repo_id:            t.id,
              name_override:      (t.info && t.name !== t.info.name) ? t.name : null,
              is_linked_worktree: !!t.isLinkedWorktree,
            })),
        applyTabMeta: (repoId, meta) => {
          tabsStore.updateTab(repoId, {
            ...(meta.nameOverride !== undefined && meta.nameOverride !== null
              ? { name: meta.nameOverride } : {}),
            ...(meta.isLinkedWorktree !== undefined
              ? { isLinkedWorktree: meta.isLinkedWorktree } : {}),
          });
        },
      });

      workspacesStore.setupListeners();
      // Bridges Tauri progress events (pull/workspace bulk/linked-WT sync) into
      // the OperationsOverlay store.  Returns a teardown closure — we don't
      // call it here because AppShell is mounted for the app's lifetime.
      setupOperationBridge();
    } finally {
      tabsStore.endInit();
    }
  });

  // Listen for open-recent events from MenuBar
  $effect(() => {
    function onOpenRecent(e: Event) {
      handleOpenRepo((e as CustomEvent<string>).detail);
    }
    document.addEventListener('open-recent', onOpenRecent);
    return () => document.removeEventListener('open-recent', onOpenRecent);
  });

  // ── Window focus / visibility tracking ───────────────────────────────────────
  // Tells the backend whether the window currently has focus so that:
  //  - Focus-gated plugin schedulers (only_when_focused = true) can skip firing.
  //  - EcoQoS / Efficiency Mode is activated on Windows (platform.rs).
  //
  // We use Tauri's native onFocusChanged() instead of DOM focus/blur events
  // because WebView2 on Windows does NOT reliably fire window.blur when the
  // window is minimized — it depends on Win32 focus messages (WM_SETFOCUS /
  // WM_KILLFOCUS) which Tauri wires up at the native level.
  $effect(() => {
    let unlisten: (() => void) | undefined;

    function applyFocus(focused: boolean, source: string) {
      const t0 = performance.now();
      console.log(`[focus] >>> applyFocus focused=${focused} source=${source} t=${t0.toFixed(1)}`);
      uiStore.setAppFocused(focused);
      const tInvoke = performance.now();
      invoke('set_app_focus', { focused })
        .then(() => {
          console.log(`[focus] set_app_focus(${focused}) ipc round-trip=${(performance.now() - tInvoke).toFixed(1)}ms`);
        })
        .catch((e: unknown) => console.error(`[focus] set_app_focus(${focused}) -> ERROR`, e));
      // Re-classify tombstoned tabs on regain-focus — covers the typical
      // "user remounted a drive / reconnected to VPN" scenario without
      // needing a manual Retry click.  Cheap when no tombstones exist.
      if (focused && missingConfig.revalidate_on_focus) {
        const tombstones = tabsStore.tabs.filter(t => t.tombstone).length;
        const tReval = performance.now();
        console.log(`[focus] revalidateTombstonedTabs candidates=${tombstones}`);
        void revalidateTombstonedTabs().then(() => {
          console.log(`[focus] revalidateTombstonedTabs done in ${(performance.now() - tReval).toFixed(1)}ms`);
        });
      }
      // Measure latency from the focus event to the next browser paint.
      // This is the visible "freeze" duration the user actually perceives —
      // captures rendering / compositor wake-up plus any synchronous JS
      // microtasks queued by the focus handler.
      if (focused) {
        const tRaf = performance.now();
        requestAnimationFrame(() => {
          console.log(`[focus] first rAF after focus=true: ${(performance.now() - tRaf).toFixed(1)}ms (since handler entry: ${(performance.now() - t0).toFixed(1)}ms)`);
        });
      }
    }

    // Register native window focus listener (async, resolves immediately).
    getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => applyFocus(focused, 'tauri:onFocusChanged'))
      .then(fn => {
        console.log('[focus] onFocusChanged listener registered');
        unlisten = fn;
      })
      .catch((e: unknown) => {
        console.warn('[focus] onFocusChanged unavailable, falling back to DOM events', e);
        function updateFocus() {
          applyFocus(document.visibilityState === 'visible' && document.hasFocus(), 'dom:fallback');
        }
        window.addEventListener('focus', updateFocus);
        window.addEventListener('blur',  updateFocus);
        document.addEventListener('visibilitychange', updateFocus);
        unlisten = () => {
          window.removeEventListener('focus', updateFocus);
          window.removeEventListener('blur',  updateFocus);
          document.removeEventListener('visibilitychange', updateFocus);
        };
      });

    // Send the initial focused state immediately (before any event fires).
    applyFocus(document.hasFocus(), 'init');

    return () => unlisten?.();
  });

  // ── Active tab sync ───────────────────────────────────────────────────────────
  // Keeps the backend informed of the currently visible tab so that
  // arbor.repo.fetch_active_tab() knows which repo to operate on.
  $effect(() => {
    const tabId = tabsStore.activeTabId;
    invoke('set_active_tab', { tabId }).catch(() => {});
  });

  // ── Security provider probe ───────────────────────────────────────────────────
  // Runs on every tab activation so the ActivityBar's Security icon and the
  // StatusBar quick-overlay can decide whether to render before the user
  // even opens the panel. The probe result is cached per tab id so this is
  // a no-op for tabs we've already evaluated.
  //
  // When the probe says "supported" we also pre-load the summary so the
  // StatusBar chip can render its severity pills (C:n H:n M:n) without
  // forcing the user to open the panel first.
  $effect(() => {
    const id = tabsStore.activeTabId;
    if (!id) return;
    securityStore
      .probeSupport(id)
      .then(ok => {
        if (ok && securityStore.snapshotTabId !== id) {
          securityStore.loadSummary(id).catch(() => {});
        }
      })
      .catch(() => {});
  });

  // ── Plugin-driven security refresh ──────────────────────────────────────────
  // `arbor.security.refresh_active_tab()` (used by the security-auto-refresh
  // plugin) emits `arbor://security-refresh { tab_id, summary }` after a
  // successful background fetch. Plug the fresh summary directly into the
  // store — no second IPC needed, the StatusBar pills and SecurityPanel
  // counters update on the next reactive tick.
  // Coalesce security-refresh per tab — if the security-auto-refresh plugin
  // fans out updates for multiple tabs in quick succession (or the WebView
  // drains a backlog after a focus change), apply only the latest summary
  // per tab on the next frame.
  const applySecurityCoalesced = coalesceLatestByKey<{ tab_id: string; summary: SecuritySummary }>(
    ({ tab_id, summary }) => securityStore.applySummary(tab_id, summary),
    (e) => e.tab_id,
  );

  $effect(() => {
    return setupTauriListeners([
      {
        event: 'arbor://security-refresh',
        handler: (e: { payload: { tab_id: string; summary: SecuritySummary } }) => {
          const { tab_id, summary } = e.payload ?? {};
          if (!tab_id || !summary) return;
          applySecurityCoalesced({ tab_id, summary });
        },
      },
      {
        // The json-studio plugin emits this via `arbor.json_studio.open(...)`.
        // We open the inspector modal here rather than from the plugin so the
        // host owns Tree/text rendering + simd-json-backed lazy navigation.
        event: 'arbor://json-studio-open',
        handler: async (e: { payload: { text?: string; path?: string; title?: string | null } }) => {
          const p = e.payload ?? {};
          if (!p.text && !p.path) return;
          await jsonStudioStore.openDoc({ text: p.text, path: p.path, title: p.title ?? null });
        },
      },
      {
        // RON Studio plugin → modal. Same shape as the JSON-studio path; the
        // host owns Tree/text/diff/schema rendering backed by the `ron`
        // crate (parse/serialise) and `syn` (cross-crate schema walking).
        event: 'arbor://ron-studio-open',
        handler: async (e: { payload: { text?: string; path?: string; title?: string | null } }) => {
          const p = e.payload ?? {};
          if (!p.text && !p.path) return;
          await ronStudioStore.openDoc({ text: p.text, path: p.path, title: p.title ?? null });
        },
      },
      {
        // TOML Studio plugin → modal. Backed by `toml_edit` host-side
        // (lossless comments / whitespace round-trip).
        event: 'arbor://toml-studio-open',
        handler: async (e: { payload: { text?: string; path?: string; title?: string | null } }) => {
          const p = e.payload ?? {};
          if (!p.text && !p.path) return;
          await tomlStudioStore.openDoc({ text: p.text, path: p.path, title: p.title ?? null });
        },
      },
      {
        // YAML Studio plugin → modal. Read-only navigation in Phase
        // 5.a (the host parses through `serde_yml` and projects to a
        // JSON shape for the tree / JSONPath query). Lossless edit +
        // cross-refs + schema land in 5.b / 5.c.
        event: 'arbor://yaml-studio-open',
        handler: async (e: { payload: { text?: string; path?: string; title?: string | null } }) => {
          const p = e.payload ?? {};
          if (!p.text && !p.path) return;
          await yamlStudioStore.openDoc({ text: p.text, path: p.path, title: p.title ?? null });
        },
      },
      {
        // `.properties` Studio plugin → modal. Lossless line-based
        // editor (Phase 6) — every byte of the source survives a
        // round-trip including comments, blank lines, continuation
        // backslashes and Unicode escapes.
        event: 'arbor://properties-studio-open',
        handler: async (e: { payload: { text?: string; path?: string; title?: string | null } }) => {
          const p = e.payload ?? {};
          if (!p.text && !p.path) return;
          await propertiesStudioStore.openDoc({ text: p.text, path: p.path, title: p.title ?? null });
        },
      },
    ]);
  });

  // ── Background-fetch / link-sync graph refresh ──────────────────────────────
  // The auto-fetch plugin emits arbor://graph-refresh after a successful fetch.
  // The Linked Worktrees orchestrator emits it for every tab whose HEAD just
  // changed because of a propagated checkout — including the initiator — and
  // includes the new branch name so the TabBar chip stays in sync without an
  // extra round-trip.
  //
  // Important: we ALWAYS invalidate the affected tab's cache, even when it's
  // not the active tab.  Otherwise the user would see stale state when they
  // switch to that tab later (the cache would re-load its old snapshot).
  // Coalesce graph-refresh per tab.  A workspace-fetch + several link-sync
  // propagations can each emit one refresh per affected tab in close
  // succession; we only need the latest payload per tab to converge on
  // the new state.
  const graphRefreshCoalesced = coalesceLatestByKey<{ tab_id: string; current_branch?: string | null }>(
    ({ tab_id, current_branch }) => {
      if (current_branch !== undefined && current_branch) {
        tabsStore.updateTab(tab_id, { currentBranch: current_branch });
      }
      if (tab_id === tabsStore.activeTabId) {
        void cacheStore.refreshIfChanged(tab_id);
      } else {
        cacheStore.invalidate(tab_id);
      }
    },
    (e) => e.tab_id,
  );

  $effect(() => {
    return setupTauriListeners([
      {
        event: 'arbor://graph-refresh',
        handler: (e: { payload: { tab_id: string; current_branch?: string | null } }) => {
          const tabId = e.payload.tab_id;
          if (!tabId) return;
          graphRefreshCoalesced(e.payload);
        },
      },
      {
        // Plugins can request a registered repo be opened as a tab via
        // arbor.tabs.open_repo(repo_id). Mirrors WorkspaceManagementModal.openRepoTab.
        event: 'arbor://open-repo-tab',
        handler: async (e: { payload: { repo_id: string; path: string } }) => {
          const { repo_id, path } = e.payload ?? {};
          if (!repo_id || !path) return;
          const existing = tabsStore.tabs.find(t => t.id === repo_id);
          if (existing) { tabsStore.setActive(existing.id); return; }
          try {
            const info = await openRepo(path, repo_id);
            tabsStore.addTab(info);
            uiStore.addRecentRepo(path);
          } catch (err) {
            uiStore.showToast(`Failed to open repo: ${err}`, 'error');
          }
        },
      },
    ]);
  });

  // ── Ticket chip click → open issue detail ────────────────────────────────────
  // Dispatched by TicketChip clicks in CommitGraph / CommitDetailPanel.
  $effect(() => {
    async function handleViewIssue(e: Event) {
      const { tracker, ticketId } = (e as CustomEvent<{ tracker: string; ticketId: string }>).detail;
      if (tracker === 'linear' || tracker === 'jira') {
        uiStore.setActiveSidebarSection('issues');
        try {
          const issue = tracker === 'jira'
            ? await jiraGetIssue(ticketId)
            : await linearGetIssue(ticketId);
          issuesStore.selectIssue(issue);
        } catch {
          uiStore.showToast(`Issue ${ticketId} not found`, 'error');
        }
      } else {
        // GitHub / GitLab — no native detail view yet, copy ID as fallback.
        await copyToClipboard(ticketId, { successToast: `${ticketId} copied to clipboard` });
      }
    }
    window.addEventListener('arbor:view-issue', handleViewIssue);
    return () => window.removeEventListener('arbor:view-issue', handleViewIssue);
  });

  // ── Command Palette integration events ──────────────────────────────────────
  // The palette dispatches window events for actions that depend on local
  // AppShell state (modals, panel toggles). We centralise the handlers here.
  $effect(() => {
    function onOpenRepoEvt()      { handleOpenRepo(); }
    function onCloneRepo()        { cloneModalOpen = true; }
    function onOpenMrDetail(e: Event) {
      const mr = (e as CustomEvent<{ mr: MergeRequest }>).detail?.mr;
      if (mr) openMrDetail(mr);
    }
    function onCreateMrEvt()      { openCreateMr(); }
    function onReloadRepo() {
      const tabId = tabsStore.activeTabId;
      if (!tabId) return;
      cacheStore.invalidate(tabId);
      graphStore.refresh();
      uiStore.showToast('Repository reloaded', 'success', 1800);
    }
    function onRenameBranchEvt(e: Event) {
      const b = (e as CustomEvent<{ branch: BranchInfo }>).detail?.branch;
      if (b) renameBranchTarget = b;
    }
    async function onOpenWsRepoEvt(e: Event) {
      const detail = (e as CustomEvent<{
        repoId: string; path: string; sourceWsId: string | null;
      }>).detail;
      if (!detail) return;
      await handleOpenWsRepo(detail);
    }
    function onManageWorkspacesEvt() { workspaceManagerOpen = true; }
    function onCreateWorkspaceEvt()  { createWorkspaceOpen = true; }
    window.addEventListener('arbor:open-repo',         onOpenRepoEvt);
    window.addEventListener('arbor:clone-repo',        onCloneRepo);
    window.addEventListener('arbor:init-repo',         onOpenRepoEvt); // picker handles non-git folders
    window.addEventListener('arbor:open-mr-detail',    onOpenMrDetail);
    window.addEventListener('arbor:create-mr',         onCreateMrEvt);
    window.addEventListener('arbor:reload-repo',       onReloadRepo);
    window.addEventListener('arbor:rename-branch',     onRenameBranchEvt);
    window.addEventListener('arbor:open-ws-repo',      onOpenWsRepoEvt);
    window.addEventListener('arbor:manage-workspaces', onManageWorkspacesEvt);
    window.addEventListener('arbor:create-workspace',  onCreateWorkspaceEvt);
    return () => {
      window.removeEventListener('arbor:open-repo',         onOpenRepoEvt);
      window.removeEventListener('arbor:clone-repo',        onCloneRepo);
      window.removeEventListener('arbor:init-repo',         onOpenRepoEvt);
      window.removeEventListener('arbor:open-mr-detail',    onOpenMrDetail);
      window.removeEventListener('arbor:create-mr',         onCreateMrEvt);
      window.removeEventListener('arbor:reload-repo',       onReloadRepo);
      window.removeEventListener('arbor:rename-branch',     onRenameBranchEvt);
      window.removeEventListener('arbor:open-ws-repo',      onOpenWsRepoEvt);
      window.removeEventListener('arbor:manage-workspaces', onManageWorkspacesEvt);
      window.removeEventListener('arbor:create-workspace',  onCreateWorkspaceEvt);
    };
  });

  // CommandPalette delegates workspace-repo opens to this handler so the
  // state mutations (addTab, setActive, markCrossWs) run in AppShell's
  // stable reactivity context — the palette is unmounting at that point
  // and doing it there was leaving the WelcomeScreen on-screen.
  async function handleOpenWsRepo(detail: {
    repoId: string; path: string; sourceWsId: string | null;
  }) {
    const existing = tabsStore.tabs.find(t => t.id === detail.repoId);
    if (existing) { tabsStore.setActive(existing.id); return; }
    if (detail.sourceWsId) {
      workspacesStore.markCrossWs(detail.repoId, detail.sourceWsId);
    }
    try {
      const info = await openRepo(detail.path, detail.repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(detail.path);
    } catch (err) {
      uiStore.showToast(`Failed to open repo: ${err}`, 'error');
    }
  }

  // Reset staging/detail panels when switching tabs so stale content is not shown.
  // Also reload bisect state for the new tab.
  let _prevTabId: string | null = null;
  $effect(() => {
    const tabId = tabsStore.activeTabId;
    if (_prevTabId !== null && tabId !== _prevTabId) {
      const bs = uiStore.activeBottomSection;
      if (bs === 'stage' || bs === 'detail') {
        uiStore.setActiveBottomSection(null);
      }
      bisectStore.clear();
    }
    if (tabId) bisectStore.load(tabId);
    _prevTabId = tabId;
  });

  // Global keybindings
  $effect(() => {
    function onKeydown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        // If any Modal-component-based modal is mounted, let its own ESC
        // handler (Modal.svelte → onClose) close the topmost one. Touching
        // our state flags here would close BOTH the topmost AND the
        // underlying modal (e.g. ESC on a FilePicker opened from inside
        // ThemeEditor would also close the editor). Modal already routes
        // ESC only to the topmost via its modalStack guard.
        if (hasOpenModal()) return;
        // Below: overlays that do NOT use Modal (command palette, recent
        // quick switch, plugin form host, search bar) still need an ESC
        // fallback here.
        if (uiStore.recentQuickSwitchOpen)  { e.preventDefault(); uiStore.setRecentQuickSwitchOpen(false); return; }
        if (uiStore.commandPaletteOpen)     { e.preventDefault(); uiStore.setCommandPaletteOpen(false); return; }
        if (pluginStore.pendingForm)        { e.preventDefault(); pluginStore.clearPendingForm(); return; }
        if (uiStore.searchVisible)          { e.preventDefault(); uiStore.setSearchVisible(false); return; }
        // Panel-level fallback: only fire when no Modal is mounted.
        // Without this guard, ESC inside a child modal of the Plugin
        // Manager (e.g. Marketplace, Plugin Info, …) would also pop
        // the Plugin Manager itself because the panel is non-graph.
        // The child Modal's own ESC handler closes it correctly via
        // the modal stack; AppShell shouldn't double-up.
        if (uiStore.activePanel !== 'graph' && !hasOpenModal()) {
          e.preventDefault();
          uiStore.setPanel('graph');
          return;
        }
        return;
      }

      // "Show keyboard inputs" overlay must work from ANY context — including
      // while a modal is open — because the user is typically recording a
      // demo when they want to flip it on/off.  Handle it here, before the
      // modal-open guard rejects everything else.
      if (matchesBinding(e, keybindingsStore.getBinding('toggle_keystrokes'))) {
        e.preventDefault();
        keystrokesStore.toggle();
        uiStore.showToast(
          keystrokesStore.enabled ? 'Keyboard inputs overlay on' : 'Keyboard inputs overlay off',
          'info',
          1800,
        );
        return;
      }

      // Don't fire global shortcuts while a modal is open — pressing Ctrl+R
      // (or any other binding) should not pile a second modal on top of the
      // current one.  We bail BEFORE plugin keybindings too, so plugin
      // shortcuts don't sneak through either.
      const modalOpen =
        openPickerOpen ||
        cloneModalOpen ||
        initModalOpen ||
        createWorkspaceOpen ||
        workspaceManagerOpen ||
        uiStore.repoBrowserOpen ||
        uiStore.marketplaceOpen ||
        statsOverlayOpen ||
        themeEditorOpen ||
        uiStore.recentQuickSwitchOpen ||
        uiStore.commandPaletteOpen ||
        jsonStudioStore.open ||
        ronStudioStore.open ||
        tomlStudioStore.open ||
        yamlStudioStore.open ||
        uiStore.mergeModalOpen ||
        uiStore.stashConflictModalOpen ||
        uiStore.checkoutConflictModalOpen ||
        pluginStore.pendingForm !== null ||
        pluginPickFile !== null ||
        mrModalOpen ||
        createMrOpen ||
        renameBranchTarget !== null ||
        onboardingStore.open;
      if (modalOpen) return;

      // Check plugin keybindings first (they take priority over unbound app keys).
      const pluginKb = contributionStore.forPoint('arbor:keybinding')
        .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
        .map(c => {
          const p = c.payload as { key?: string; ctrl?: boolean; shift?: boolean; alt?: boolean; action?: string; description?: string };
          return { plugin_name: c.plugin_name, key: p.key ?? '', ctrl: !!p.ctrl, shift: !!p.shift, alt: !!p.alt, action: p.action ?? '', description: p.description ?? '' };
        })
        .find(kb => matchesBinding(e, { key: kb.key, ctrl: kb.ctrl, shift: kb.shift, alt: kb.alt, description: kb.description, group: '' }));
      if (pluginKb) {
        e.preventDefault();
        firePluginAction(pluginKb.plugin_name, pluginKb.action, '{}').catch(() => {});
        return;
      }

      const action = keybindingsStore.matchAction(e);
      if (!action) return;
      e.preventDefault();

      // While on a non-graph panel (Plugin Manager, Settings, Docs) only
      // panel-switch and tab-navigation shortcuts are allowed.  Pressing
      // e.g. the repo-browser shortcut from inside Plugin Manager should
      // NOT pop a Repo Browser modal on top of it — the user can Escape
      // back to the graph first.
      if (uiStore.activePanel !== 'graph') {
        const ALLOWED_OFF_GRAPH = new Set([
          'settings', 'plugins', 'open_marketplace', 'toggle_docs',
          'command_palette', 'open_project', 'open_from_workspace',
          'next_tab', 'prev_tab', 'close_tab',
          // Bottom-panel toggles are independent from the active main panel —
          // the user should be able to flip them from inside Settings, Plugin
          // Manager, Docs, etc.
          'plugin_logs',
          'toggle_bottom_panel',
          // Layout focus cycling is global by design.
          'cycle_focus', 'cycle_focus_reverse',
        ]);
        if (!ALLOWED_OFF_GRAPH.has(action)) return;
      }

      switch (action) {
        case 'open_repo':        handleOpenRepo(); break;
        case 'clone_repo':       window.dispatchEvent(new CustomEvent('arbor:clone-repo')); break;
        case 'init_repo':        window.dispatchEvent(new CustomEvent('arbor:init-repo')); break;
        case 'close_tab':        if (tabsStore.activeTab) tabsStore.removeTab(tabsStore.activeTab.id); break;
        case 'next_tab':         tabsStore.nextTab(); break;
        case 'prev_tab':         tabsStore.prevTab(); break;
        case 'toggle_sidebar':   uiStore.toggleSidebarVisibility(); break;
        case 'toggle_bottom_panel': uiStore.toggleBottomVisibility(); break;
        // Sidebar section toggles — silently no-op when the matching
        // ActivityBar button has been hidden via Settings → Customize
        // Activity Bar (mirrors IntelliJ Alt+1..9 behavior).
        case 'toggle_branches_sidebar': uiStore.toggleSidebarSectionIfVisible('branches'); break;
        case 'toggle_mr_sidebar':       uiStore.toggleSidebarSectionIfVisible('mr');       break;
        case 'toggle_files_sidebar':    uiStore.toggleSidebarSectionIfVisible('files');    break;
        case 'toggle_gitflow_sidebar':  uiStore.toggleSidebarSectionIfVisible('gitflow');  break;
        case 'toggle_issues_sidebar':   uiStore.toggleSidebarSectionIfVisible('issues');   break;
        case 'toggle_reflog_sidebar':   uiStore.toggleSidebarSectionIfVisible('reflog');   break;
        case 'toggle_stats_sidebar':    uiStore.toggleSidebarSectionIfVisible('stats');    break;
        case 'toggle_security_sidebar': uiStore.toggleSidebarSectionIfVisible('security'); break;
        case 'toggle_pipelines_panel':  uiStore.toggleBottomSectionIfVisible('pipelines'); break;
        case 'settings':         uiStore.setPanel(uiStore.activePanel === 'settings' ? 'graph' : 'settings'); break;
        case 'plugins':          uiStore.setPanel(uiStore.activePanel === 'plugins'  ? 'graph' : 'plugins');  break;
        case 'open_marketplace': uiStore.marketplaceOpen ? uiStore.closeMarketplace() : uiStore.openMarketplace(); break;
        case 'command_palette':      uiStore.toggleCommandPalette(); break;
        case 'open_project':         uiStore.openCommandPaletteWithVerb('open-project'); break;
        case 'open_from_workspace':  uiStore.openCommandPaletteWithVerb('open-from-workspace'); break;
        case 'search':           uiStore.setSearchVisible(!uiStore.searchVisible); break;
        case 'stage_view':       uiStore.toggleBottomSection('stage'); break;
        case 'toggle_docs':      uiStore.setPanel(uiStore.activePanel === 'docs' ? 'graph' : 'docs'); break;
        case 'toggle_terminal':  uiStore.toggleBottomSection('terminal'); break;
        case 'plugin_logs':      uiStore.toggleBottomSection('plugin-logs'); break;
        case 'new_terminal':     openNewTerminal(); break;
        case 'fetch':            window.dispatchEvent(new CustomEvent('arbor:fetch')); break;
        case 'refresh_graph':    window.dispatchEvent(new CustomEvent('arbor:fetch')); break;
        case 'workspace_manager': workspaceManagerOpen = true; break;
        case 'pull':             window.dispatchEvent(new CustomEvent('arbor:pull')); break;
        case 'push':             window.dispatchEvent(new CustomEvent('arbor:push')); break;
        case 'new_branch':       window.dispatchEvent(new CustomEvent('arbor:new-branch')); break;
        case 'stash':            window.dispatchEvent(new CustomEvent('arbor:stash')); break;
        case 'jump_to_head':     window.dispatchEvent(new CustomEvent('arbor:jump-to-head')); break;
        case 'focus_graph':      window.dispatchEvent(new CustomEvent('arbor:focus-graph')); break;
        case 'cycle_focus':         cycleFocus(1);  break;
        case 'cycle_focus_reverse': cycleFocus(-1); break;
        case 'next_chunk':       window.dispatchEvent(new CustomEvent('arbor:next-chunk')); break;
        case 'prev_chunk':       window.dispatchEvent(new CustomEvent('arbor:prev-chunk')); break;
        case 'open_recent':      uiStore.toggleRecentQuickSwitch(); break;
        case 'repo_browser':     uiStore.openRepoBrowser(); break;
        case 'toggle_right_sidebar': uiStore.toggleRightSidebarVisibility(); break;
      }

    }
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });

  // ── Layout-zone focus cycling (F6 / Shift+F6) ──────────────────────────────
  // Lets the user move focus across the major UI regions from the keyboard
  // without dedicated shortcuts for each one. The order roughly mirrors the
  // visual flow: top bar → tabs → left rails → sidebar → graph → bottom
  // panel → right rails → status bar. Zones whose root element isn't in the
  // DOM (collapsed sidebar / hidden bottom panel / …) are skipped.
  type FocusZone = { name: string; selector: string };
  const FOCUS_ZONES: FocusZone[] = [
    { name: 'titlebar',       selector: '.titlebar' },
    { name: 'tabs',           selector: '.tabbar-wrap' },
    { name: 'activity-left',  selector: '.activity-bar[data-side="left"]' },
    { name: 'sidebar',        selector: '.sidebar-wrap' },
    { name: 'graph',          selector: '.graph-area' },
    { name: 'bottom',         selector: '.bottom-wrap' },
    { name: 'right-sidebar',  selector: '.right-sidebar-wrap' },
    { name: 'activity-right', selector: '.activity-bar[data-side="right"]' },
    { name: 'statusbar',      selector: '.statusbar' },
  ];
  const FOCUSABLE_SEL =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]),' +
    ' textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function focusZone(zone: FocusZone): boolean {
    // Graph uses its existing event hook so the scrollEl gets focus (which
    // is what enables the arrow / Page / Home / End graph navigation).
    if (zone.name === 'graph') {
      window.dispatchEvent(new CustomEvent('arbor:focus-graph'));
      return true;
    }
    const root = document.querySelector<HTMLElement>(zone.selector);
    if (!root) return false;
    const focusable = root.querySelector<HTMLElement>(FOCUSABLE_SEL);
    if (focusable) { focusable.focus(); return true; }
    // No natural focusable inside — focus the wrapper itself.
    if (root.tabIndex < 0) root.tabIndex = -1;
    root.focus();
    return true;
  }

  function cycleFocus(direction: 1 | -1) {
    const zones = FOCUS_ZONES.filter(z => document.querySelector(z.selector) !== null);
    if (zones.length === 0) return;
    const active = document.activeElement as HTMLElement | null;
    let currentIdx = -1;
    for (let i = 0; i < zones.length; i++) {
      const el = document.querySelector(zones[i].selector);
      if (el && active && el.contains(active)) { currentIdx = i; break; }
    }
    const nextIdx = currentIdx === -1
      ? (direction === 1 ? 0 : zones.length - 1)
      : (currentIdx + direction + zones.length) % zones.length;
    focusZone(zones[nextIdx]);
  }

  async function openNewTerminal() {
    // Ensure terminal panel is visible
    if (uiStore.activeBottomSection !== 'terminal') {
      uiStore.setActiveBottomSection('terminal');
    }
    try {
      const cwd = tabsStore.activeTab?.path;
      const info = await terminalCreate({ cwd });
      terminalStore.addTab(info.id, info.shell, info.cwd);
    } catch (err) {
      uiStore.showToast(`Failed to open terminal: ${err}`, 'error');
    }
  }

  // Plugin Tauri event listeners
  $effect(() => {
    // All plugin Tauri event listeners are registered via setupTauriListeners so
    // they are safe against Svelte 5 dev-mode double-effect invocation: if the
    // cleanup fires before the async listen() Promise resolves, the promise
    // callback immediately unlistens the ghost listener.
    const unlistenPlugin = setupTauriListeners([
      {
        event: 'plugin:toast',
        handler: (e: { payload: { plugin: string; message: string; level: string } }) => {
          const { plugin, message, level } = e.payload;
          uiStore.showToast(`[${plugin}] ${message}`, (level as any) ?? 'info');
        },
      },
      {
        event: 'plugin:form',
        handler: (e: { payload: PluginFormConfig }) => {
          pluginStore.setPendingForm(e.payload);
        },
      },
      {
        // In-app notification center (from arbor.notify). The plugin can mute
        // either channel via `toast = false` (no transient pop) or
        // `persist = false` (skip the bell — useful for "kicked off"
        // chatter that the user doesn't need to read again later).
        // Both default to true → existing call sites keep their behavior.
        //
        // A persisted notification ALREADY surfaces in the bottom-right
        // transient stack via `notificationsStore.transient` (NotificationItem
        // is interleaved with toasts in `feedItems`), so calling `showToast`
        // on top would render two cards for one event. Only fall back to the
        // toast path when the caller opted OUT of persistence — that's the
        // one mode where a transient pop is the only channel.
        event: 'plugin:notification',
        handler: (e: { payload: { plugin: string; title: string; message: string; level: string; toast?: boolean; persist?: boolean; action?: import('$lib/stores/notifications.svelte').NotificationAction } }) => {
          const { plugin, title, message, level, toast, persist, action } = e.payload;
          const showToast   = toast   !== false;
          const persistBell = persist !== false;
          if (persistBell) {
            notificationsStore.add(title, message, (level as any) ?? 'info', plugin, action);
          } else if (showToast) {
            const toastMsg = message ? `${title} — ${message}` : title;
            uiStore.showToast(toastMsg, (level as any) ?? 'info', 5000);
          }
        },
      },
      {
        // File/folder/save picker opened by a plugin via `arbor.ui.pick_file`.
        // The payload carries the plugin name + the requested options + a
        // callback action name; we show FilePickerModal and round-trip the
        // chosen path back as a plugin action (empty path on cancel).
        event: 'plugin:pick-file',
        handler: (e: { payload: PluginPickFile }) => {
          pluginPickFile = e.payload;
        },
      },
      {
        // `arbor.ui.copy_to_clipboard(text)` — browser clipboard API runs in
        // the webview context, not Rust, so we route through an event. The
        // plugin can customise the confirmation toast via the `toast` field.
        event: 'plugin:ui-clipboard-write',
        handler: async (e: { payload: { plugin: string; text: string; toast?: string } }) => {
          const { text, toast } = e.payload;
          await copyToClipboard(text, {
            successToast: toast ?? 'Copied to clipboard',
            errorToast: true,
          });
        },
      },
      {
        // `arbor.ui.show_pipeline_run(run_id)` — plugins deep-link into a
        // pipeline run. We open the standalone `PipelineRunDetailModal`
        // (z-index 1100) directly, NOT the bottom Pipelines panel, because
        // the caller is typically already inside a modal and the bottom
        // panel would be obscured. The detail modal stacks above any open
        // plugin form so the user sees the graph + output log immediately.
        event: 'plugin:ui-show-pipeline-run',
        handler: (e: { payload: { plugin: string; run_id: string } }) => {
          const { run_id } = e.payload;
          if (!run_id) return;
          pipelinesStore.setActiveRun(run_id);
        },
      },
      {
        // `arbor.ui.open_job_output(job_id)` — plugins (typically run-monitor)
        // surface a specific job's streaming output. We mirror what JobsOverlay
        // does on click: load the buffer, mark the job active, swap the bottom
        // section to 'jobs'. If the job has been purged from the registry the
        // panel renders its own empty state.
        event: 'plugin:ui-open-job-output',
        handler: async (e: { payload: { plugin: string; job_id: string } }) => {
          const { job_id } = e.payload;
          if (!job_id) return;
          await jobsStore.loadOutput(job_id);
          jobsStore.setActiveJob(job_id);
          uiStore.setActiveBottomSection('jobs' as any);
        },
      },
      {
        // `arbor.ui.open_panel(panel_id)` — programmatically reveal one of
        // the plugin's own sidebar panels. We resolve the registered side +
        // position from the sidebar contribution so the plugin doesn't have
        // to know its own placement. Unknown panel id → no-op (avoids
        // surprising the user with a panel they never registered).
        event: 'plugin:ui-open-panel',
        handler: (e: { payload: { plugin: string; panel_id: string } }) => {
          const { plugin, panel_id } = e.payload;
          if (!plugin || !panel_id) return;
          const section = contributionStore.forPoint(SIDEBAR_POINT)
            .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
            .map(parseSidebarSection)
            .find(s => s.plugin_name === plugin && s.id === panel_id);
          if (!section) return;
          const key = `plugin:${plugin}:${panel_id}`;
          if (section.position === 'bottom') {
            uiStore.setActiveBottomSection(key as any);
          } else if (section.side === 'right') {
            uiStore.setActiveRightSidebar(key);
          } else {
            uiStore.setActiveSidebarSection(key);
          }
        },
      },
    ]);

    // Combo options updates + plugin reload events
    const unlistenCombo = pluginStore.setupListeners();

    // Reconcile the frontend disabled-plugins Set with the backend's
    // plugin_states.json — newly discovered plugins start disabled and the
    // localStorage Set must reflect that before any contribution filter runs.
    listPluginInfo()
      .then(infos => pluginStore.syncFromInfos(infos))
      .catch(() => { /* backend not ready yet — PluginPanel will sync later */ });

    // Cross-plugin contributions + tree snapshots + custom icons
    const unlistenContributions = contributionStore.setupListeners();
    contributionStore.reloadAll();

    // Container model (Phase 2 — ContributableModal)
    const unlistenContainers = containerStore.setupListeners();
    containerStore.reloadDefs();

    // deps-explorer modal — listens for tree-state contribution updates whose
    // sidebar id matches the `deps:<request_id>` convention pushed by the
    // deps-explorer plugin and pops the IntelliJ-style modal.
    const unlistenDepsExplorer = depsExplorerStore.setupListeners();

    // Job events
    const unlistenJobs = jobsStore.setupListeners();
    jobsStore.load();

    // Plugin log stream
    const unlistenPluginLogs = pluginLogsStore.setupListeners();
    pluginLogsStore.load();

    // Pipeline events + initial load
    const unlistenPipelines = pipelinesStore.setupListeners();
    pipelinesStore.load();

    // Linked Worktrees (cross-project sync) events + initial load
    const unlistenLinks = linkedWorktreesStore.setupListeners();
    linkedWorktreesStore.load();

    // Stats events (result of background compute_repo_stats)
    const unlistenStats = statsStore.setupListeners();

    // Diff streaming events (progressive workdir diff loader)
    const unlistenDiffStreamPromise = diffStore.setupListeners();

    // Activity bar config — load persisted order/visibility
    activityBarConfigStore.load();

    return () => {
      unlistenPlugin();
      unlistenCombo();
      unlistenContributions();
      unlistenContainers();
      unlistenDepsExplorer();
      unlistenJobs();
      unlistenPluginLogs();
      unlistenPipelines();
      unlistenLinks();
      unlistenStats();
      unlistenDiffStreamPromise.then(fn => fn());
    };
  });

  // When the activity bar config is loaded, ensure the persisted active sidebar/bottom
  // section is still visible — if not, reset to null so the app doesn't trigger
  // hidden-panel computations (e.g. stats) on startup.
  $effect(() => {
    if (!activityBarConfigStore.loaded) return;
    const section = uiStore.activeSidebarSection;
    if (section && !activityBarConfigStore.isVisible(section)) {
      uiStore.setActiveSidebarSection(null);
    }
  });

  const hasRepo           = $derived(tabsStore.activeTab !== null);
  const activePanel       = $derived(uiStore.activePanel);

  // Merged feed for the bottom-right stack: toasts + transient notifications
  // sorted by insertion time (ascending — oldest at top, newest just above
  // the operations zone). Only freshly-added notifications appear here; the
  // full archive lives in the bell overlay. Operation cards render after
  // this loop and stay anchored at the bottom of the column.
  type FeedItem =
    | { kind: 'toast';        key: string; ts: number; value: typeof uiStore.toasts[number] }
    | { kind: 'notification'; key: string; ts: number; value: typeof notificationsStore.notifications[number] };
  const feedItems = $derived.by<FeedItem[]>(() => {
    const out: FeedItem[] = [];
    for (const t of uiStore.toasts)
      out.push({ kind: 'toast',        key: `t:${t.id}`, ts: t.addedAt,  value: t });
    for (const n of notificationsStore.transient)
      out.push({ kind: 'notification', key: `n:${n.id}`, ts: n.timestamp, value: n });
    out.sort((a, b) => a.ts - b.ts);
    return out;
  });
  const showSettings      = $derived(activePanel === 'settings');
  const showPlugins       = $derived(activePanel === 'plugins');
  const showAbout         = $derived(activePanel === 'about');
  const showDocs          = $derived(activePanel === 'docs');
  const pendingPluginForm = $derived(pluginStore.pendingForm);
  const bottomSection     = $derived(uiStore.activeBottomSection);
  const showBottomDetail  = $derived(hasRepo && bottomSection === 'detail');
  const showBottomStage   = $derived(hasRepo && bottomSection === 'stage');
  const showTerminal      = $derived(hasRepo && bottomSection === 'terminal');
  const showJobOutput     = $derived(bottomSection === 'jobs' && jobsStore.activeJobId !== null);
  const showPipelines     = $derived(hasRepo && bottomSection === 'pipelines');
  const showPluginLogs    = $derived(bottomSection === 'plugin-logs');
  const showPluginBottom  = $derived(
    typeof bottomSection === 'string' && bottomSection.startsWith('plugin:')
  );
  const showSidebar       = $derived(hasRepo && uiStore.activeSidebarSection !== null);
  const showRightSidebar  = $derived(hasRepo && uiStore.activeRightSidebar !== null);

  /** Parse `"plugin:<name>:<id>"` into `{plugin_name, panel_id}`. */
  function parsePluginKey(key: string | null): { plugin_name: string; panel_id: string } | null {
    if (!key || !key.startsWith('plugin:')) return null;
    const rest = key.slice('plugin:'.length);
    const colon = rest.indexOf(':');
    if (colon < 0) return null;
    return { plugin_name: rest.slice(0, colon), panel_id: rest.slice(colon + 1) };
  }

  /** True when the section identified by `key` was registered with kind="tree".
   *  Falls back to false (form-DSL) for any unknown section so a stale id
   *  doesn't break rendering. */
  function isTreeKind(key: { plugin_name: string; panel_id: string } | null): boolean {
    if (!key) return false;
    const s = contributionStore.forPoint(SIDEBAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection)
      .find(sec => sec.plugin_name === key.plugin_name && sec.id === key.panel_id);
    return s?.kind === 'tree';
  }

  const leftPluginKey  = $derived(parsePluginKey(uiStore.activeSidebarSection));
  const rightPluginKey = $derived(parsePluginKey(uiStore.activeRightSidebar));
  const bottomPluginKey= $derived(parsePluginKey(typeof bottomSection === 'string' ? bottomSection : null));

  // Stats overlay state
  let statsOverlayOpen = $state(false);

  // Theme editor state
  let themeEditorOpen = $state(false);

  // MR / PR modal state
  let mrModalOpen      = $state(false);
  let mrModalMr        = $state<MergeRequest | null>(null);
  let createMrOpen     = $state(false);
  let mrCurrentBranch  = $state('');

  // Rename-branch modal state (driven by palette event)
  let renameBranchTarget = $state<BranchInfo | null>(null);

  // Tag-delete modal state (driven by palette event — sidebar handles its
  // own modal locally for the context-menu flow).
  let pendingTagDelete = $state<{ tabId: string; name: string; scope: 'local' | 'remote' } | null>(null);
  $effect(() => {
    function onDeleteTag(e: Event) {
      const d = (e as CustomEvent<{ tabId: string; name: string; scope: 'local' | 'remote' }>).detail;
      if (d?.tabId && d.name && (d.scope === 'local' || d.scope === 'remote')) {
        pendingTagDelete = { tabId: d.tabId, name: d.name, scope: d.scope };
      }
    }
    window.addEventListener('arbor:delete-tag', onDeleteTag);
    return () => window.removeEventListener('arbor:delete-tag', onDeleteTag);
  });

  // Global Git Blame modal — driven by the `arbor:show-blame` event so any
  // surface (Command Palette, plugin, future quick-action menus) can ask
  // for a blame without forcing a particular sidebar open.
  let blameTarget = $state<{ tabId: string; path: string } | null>(null);
  $effect(() => {
    function onShowBlame(e: Event) {
      const d = (e as CustomEvent<{ tabId: string; path: string }>).detail;
      if (d?.tabId && d?.path) blameTarget = { tabId: d.tabId, path: d.path };
    }
    window.addEventListener('arbor:show-blame', onShowBlame);
    return () => window.removeEventListener('arbor:show-blame', onShowBlame);
  });

  // Global Worktree Info modal — driven by the `arbor:show-worktree-info`
  // event so the Command Palette (and any future surface) can open it
  // without the Worktrees sidebar section being visible.
  let worktreeInfoTarget = $state<WorktreeInfo | null>(null);
  $effect(() => {
    function onShowWorktreeInfo(e: Event) {
      const d = (e as CustomEvent<{ worktree: WorktreeInfo }>).detail;
      if (d?.worktree) worktreeInfoTarget = d.worktree;
    }
    window.addEventListener('arbor:show-worktree-info', onShowWorktreeInfo);
    return () => window.removeEventListener('arbor:show-worktree-info', onShowWorktreeInfo);
  });

  // Workspace modal state
  let workspaceManagerOpen = $state(false);
  let createWorkspaceOpen  = $state(false);

  // Deep-link follow-up modal state.  These two host the modals that the
  // deep-link dispatcher needs but that aren't normally rendered top-level
  // (worktree creation lives inside WorktreeList; CI detail lives inside
  // PipelinesPanel).  We instantiate parallel copies here so the dispatcher
  // can open them on any tab regardless of which side panel is visible.
  let dlWorktreeState = $state<{ tabId: string; branch: string } | null>(null);
  let dlCiRun         = $state<CiRun | null>(null);
  let dlCiTabId       = $state<string>('');
  // Loading/error placeholder for deep-link follow-ups whose real modal
  // depends on a slow async fetch (MR detail, CI run lookup).  Stays open
  // until either the data lands and we swap to the real modal, or the user
  // dismisses the error state.
  let dlLoading       = $state<{ title: string; status: 'loading' | 'error'; message?: string } | null>(null);
  /** Best-effort detection of "the requested entity does not exist" — used
   *  to soften the error message vs. a generic crash. */
  function isNotFound(err: unknown): boolean {
    return /not.?found|404|does.?not.?exist|no.?such/i.test(String(err));
  }

  // ── Bottom-panel readiness signal ──────────────────────────────────────────
  // Two paths to "the bottom panel is now stable and laid out":
  //   1. The Svelte `bottomSlide` transition just finished — `onintroend`
  //      flips `bottomTransitioning` off and fires `notifyBottomPanelReady`.
  //   2. `activeBottomSection` changed BUT no transition fired (panel was
  //      already mounted, only the inner {#if} swapped).  The effect below
  //      catches this case: after 2 frames, if we're not transitioning,
  //      the panel is ready right now.
  // The deep-link dispatcher subscribes to `awaitBottomPanelReady` BEFORE
  // calling `setActiveBottomSection`, so the signal is never missed.
  let bottomTransitioning = $state(false);
  $effect(() => {
    uiStore.activeBottomSection;          // track changes
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!bottomTransitioning) uiStore.notifyBottomPanelReady();
      });
    });
  });

  function openMrDetail(mr: MergeRequest) {
    mrModalMr   = mr;
    mrModalOpen = true;
  }
  function closeMrDetail() {
    mrModalOpen = false;
    mrModalMr   = null;
    mrStore.clearDetail();
  }
  function openCreateMr() {
    // Try to get current branch name
    const tab = tabsStore.activeTab;
    mrCurrentBranch = (tab as any)?.branch ?? '';
    createMrOpen    = true;
  }
  function onMrCreated() {
    // Refresh the MR list after creation
    const tabId = tabsStore.activeTabId;
    if (tabId) mrStore.load(tabId);
  }
</script>

<!-- Boot-time splash overlay. Self-mounts at startup, listens for
     `arbor://boot-progress` / `arbor://boot-done` and removes itself when
     the host signals plugin loading is complete. -->
<BootSplash />

<div class="shell">
  <TitleBar
    onOpen={handleOpenRepo}
    onClone={() => cloneModalOpen = true}
    onInit={startInitRepoFlow}
    onOpenThemeEditor={() => themeEditorOpen = true}
    onManageWorkspaces={() => workspaceManagerOpen = true}
    onCreateWorkspace={() => createWorkspaceOpen = true}
  />

  <div class="content-area">
    {#if !hasRepo}
      <WelcomeScreen
        onOpen={handleOpenRepo}
        onOpenPath={(p) => handleOpenRepo(p)}
        onClone={() => cloneModalOpen = true}
        onManageWorkspaces={() => workspaceManagerOpen = true}
      />
    {:else}
      <div class="workspace">
        <!-- Activity Bar: always-visible left icon rail, flush to left edge -->
        <ActivityBarLeft />

        <!-- Inset panels container: gaps reveal the workspace bg (IntelliJ-style) -->
        <div class="panels">
          <!-- Sidebar: shown when a top section is active -->
          {#if showSidebar}
            <div class="sidebar-wrap"
                 transition:sidebarSlide={{ duration: animStore.dPanel }}>
              <ResizablePanel
                direction="horizontal"
                initialSize={uiStore.sidebarWidth}
                minSize={160}
                maxSize={500}
                onResize={uiStore.setSidebarWidth}
              >
                {#if uiStore.activeSidebarSection === 'gitflow'}
                  <GitFlowPanel />
                {:else if uiStore.activeSidebarSection === 'mr'}
                  <MrSidebar onOpenCreate={openCreateMr} onOpenDetail={openMrDetail} />
                {:else if uiStore.activeSidebarSection === 'issues'}
                  <IssuesSidebar />
                {:else if uiStore.activeSidebarSection === 'files'}
                  <FileTreePanel />
                {:else if uiStore.activeSidebarSection === 'reflog'}
                  <ReflogPanel />
                {:else if uiStore.activeSidebarSection === 'stats'}
                  <StatsPanel onOpenFull={() => statsOverlayOpen = true} />
                {:else if uiStore.activeSidebarSection === 'security'}
                  <SecurityPanel />
                {:else if uiStore.activeSidebarSection === 'studio'}
                  <StudioPanel />
                {:else if leftPluginKey}
                  {#if isTreeKind(leftPluginKey)}
                    <PluginTreeSidebar pluginName={leftPluginKey.plugin_name} panelId={leftPluginKey.panel_id} />
                  {:else}
                    <PluginSidebarPanel pluginName={leftPluginKey.plugin_name} panelId={leftPluginKey.panel_id} />
                  {/if}
                {:else}
                  <Sidebar />
                {/if}
              </ResizablePanel>
            </div>
          {/if}

          <!-- Main column: editor card (tabs + graph + bisect) + optional
               bottom panel card.  Separated by a visible gap that reveals
               the workspace bg, giving each a floating-card feel. -->
          <div class="main-col">
            <div class="editor-col">
              <TabBar onOpen={handleOpenRepo} />
              <div class="graph-area">
                {#if tabsStore.activeTab?.tombstone}
                  <MissingRepoState tab={tabsStore.activeTab} config={missingConfig} />
                {:else}
                  <CommitGraph />
                {/if}
              </div>

              <!-- Bisect banner: shown inside the editor card -->
              {#if tabsStore.activeTabId}
                <BisectBanner tabId={tabsStore.activeTabId} />
              {/if}
            </div>

            <!-- Bottom panel: stage, commit detail, terminal, jobs, pipelines,
                 or a plugin-registered panel.  Outer div handles open/close
                 animation; inner {#key} fades on panel switch. -->
            {#if showBottomStage || showBottomDetail || showTerminal || showJobOutput || showPipelines || showPluginLogs || showPluginBottom}
            <div class="bottom-wrap"
                 data-bottom-panel
                 transition:bottomSlide={{ duration: animStore.dPanel }}
                 onintrostart={() => { bottomTransitioning = true; }}
                 onintroend={() => { bottomTransitioning = false; uiStore.notifyBottomPanelReady(); }}
                 onoutrostart={() => { bottomTransitioning = true; }}
                 onoutroend={() => { bottomTransitioning = false; }}>
              {#if showBottomStage}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={100}
                  maxSize={600}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <StageArea />
                </ResizablePanel>
              {:else if showBottomDetail}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={100}
                  maxSize={600}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <CommitDetailPanel />
                </ResizablePanel>
              {:else if showTerminal}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={120}
                  maxSize={700}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <TerminalPanel />
                </ResizablePanel>
              {:else if showJobOutput}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={120}
                  maxSize={700}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <JobOutputPanel />
                </ResizablePanel>
              {:else if showPipelines}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={140}
                  maxSize={700}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <PipelinesPanel />
                </ResizablePanel>
              {:else if showPluginLogs}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={120}
                  maxSize={700}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  <PluginLogsPanel />
                </ResizablePanel>
              {:else if showPluginBottom && bottomPluginKey}
                <ResizablePanel
                  direction="vertical"
                  initialSize={uiStore.bottomHeight}
                  minSize={100}
                  maxSize={600}
                  onResize={uiStore.setBottomHeight}
                  reverse
                >
                  {#if isTreeKind(bottomPluginKey)}
                    <PluginTreeSidebar
                      pluginName={bottomPluginKey.plugin_name}
                      panelId={bottomPluginKey.panel_id}
                      bottomMode
                    />
                  {:else}
                    <PluginSidebarPanel
                      pluginName={bottomPluginKey.plugin_name}
                      panelId={bottomPluginKey.panel_id}
                      bottomMode
                    />
                  {/if}
                </ResizablePanel>
              {/if}
            </div>
            {/if}
          </div>

          <!-- Right sidebar panel — plugin-registered panels (side="right"). -->
          {#if showRightSidebar && rightPluginKey}
            <div class="right-sidebar-wrap"
                 transition:sidebarSlide={{ duration: animStore.dPanel }}>
              <ResizablePanel
                direction="horizontal"
                initialSize={uiStore.rightSidebarWidth}
                minSize={160}
                maxSize={500}
                onResize={uiStore.setRightSidebarWidth}
                reverse
              >
                {#if isTreeKind(rightPluginKey)}
                  <PluginTreeSidebar
                    pluginName={rightPluginKey.plugin_name}
                    panelId={rightPluginKey.panel_id}
                  />
                {:else}
                  <PluginSidebarPanel
                    pluginName={rightPluginKey.plugin_name}
                    panelId={rightPluginKey.panel_id}
                  />
                {/if}
              </ResizablePanel>
            </div>
          {/if}
        </div>

        <!-- Right ActivityBar: hidden completely when no plugin has
             registered a right-side entry. -->
        <ActivityBarRight />

      </div>
    {/if}
  </div>

  <StatusBar />

  <!-- Floating "keyboard inputs" overlay — pure presentation. Capture
       listener lives on the store and stays attached only while enabled. -->
  <KeystrokesOverlay />

  <!-- Jobs overlay (floating above statusbar) -->
  {#if uiStore.jobsOverlayOpen}
    <div transition:fly={{ y: 10, duration: animStore.dBase, easing: cubicOut }}>
      <JobsOverlay />
    </div>
  {/if}

  <!-- Notifications archive overlay (toggleable bell panel — full history;
       new notifications also live transiently in the bottom-right stack). -->
  {#if uiStore.notificationsOverlayOpen}
    <div transition:fly={{ y: 10, duration: animStore.dBase, easing: cubicOut }}>
      <NotificationsOverlay />
    </div>
  {/if}

  <!-- Security quick-overlay (floating above statusbar, click-outside) -->
  {#if uiStore.securityOverlayOpen}
    <div transition:fly={{ y: 10, duration: animStore.dBase, easing: cubicOut }}>
      <SecurityQuickOverlay />
    </div>
  {/if}

  <!-- OperationsOverlay is rendered INSIDE the bottom-right unified stack
       above (so it's always anchored at the bottom of the column). -->


  <!-- MR detail modal -->
  {#if mrModalOpen && mrModalMr}
    <MrModal
      mr={mrModalMr}
      onClose={closeMrDetail}
      onRefresh={() => { const tid = tabsStore.activeTabId; if (tid) mrStore.load(tid); }}
    />
  {/if}

  <!-- Deep-link follow-up modals (worktree creation + CI run detail) and the
       clone-confirm gate.  Rendered top-level so they don't depend on which
       sidebar/panel is currently visible. -->
  {#if deepLinkDispatcher.pendingDisabled}
    {@const p = deepLinkDispatcher.pendingDisabled}
    <DeepLinkDisabledModal
      title={p.title}
      message={p.message}
      url={p.url}
      onClose={p.onClose}
    />
  {/if}

  {#if deepLinkDispatcher.pendingActionConfirm}
    {@const p = deepLinkDispatcher.pendingActionConfirm}
    <DeepLinkActionConfirmModal
      title={p.title}
      description={p.description}
      url={p.url}
      onConfirm={p.onAccept}
      onCancel={p.onReject}
    />
  {/if}

  {#if deepLinkDispatcher.pendingClone}
    {@const p = deepLinkDispatcher.pendingClone}
    <DeepLinkConfirmModal
      url={p.url}
      actionDescription={p.description}
      reason={p.reason}
      onClose={() => deepLinkDispatcher.cancelClone()}
      onConfirmed={(repoId) => deepLinkDispatcher.confirmClone(repoId)}
    />
  {/if}

  {#if dlWorktreeState}
    <AddWorktreeModal
      tabId={dlWorktreeState.tabId}
      initialBranch={dlWorktreeState.branch}
      onClose={() => { dlWorktreeState = null; }}
      onAdded={() => { dlWorktreeState = null; }}
    />
  {/if}

  {#if dlCiRun}
    <CiPipelineDetailModal
      run={dlCiRun}
      tabId={dlCiTabId}
      onClose={() => { dlCiRun = null; }}
    />
  {/if}

  {#if dlLoading}
    <DeepLinkLoadingModal
      title={dlLoading.title}
      status={dlLoading.status}
      message={dlLoading.message}
      onClose={() => { dlLoading = null; }}
    />
  {/if}

  <!-- Create MR / PR modal -->
  {#if createMrOpen}
    <CreateMrModal
      currentBranch={mrCurrentBranch}
      onClose={() => createMrOpen = false}
      onCreated={onMrCreated}
    />
  {/if}

  <!-- Rename branch modal (triggered by palette) -->
  {#if renameBranchTarget}
    <BranchRenameModal
      branch={renameBranchTarget}
      onClose={() => renameBranchTarget = null}
      onRenamed={() => { const tid = tabsStore.activeTabId; if (tid) cacheStore.invalidate(tid); graphStore.refresh(); }}
    />
  {/if}

  <!-- Delete tag modal (triggered by palette via arbor:delete-tag) -->
  {#if pendingTagDelete}
    <DeleteTagModal
      tagName={pendingTagDelete.name}
      scope={pendingTagDelete.scope}
      onCancel={() => pendingTagDelete = null}
      onConfirm={async () => {
        const p = pendingTagDelete!;
        pendingTagDelete = null;
        await runTagDelete(p.tabId, p.name, p.scope);
      }}
    />
  {/if}

  <!-- Recent Repos quick-switch modal -->
  <RecentReposModal onOpen={(p) => handleOpenRepo(p)} />

  <!-- Dependency Explorer modal (opened by the `deps-explorer` plugin
       through `arbor.ui.tree.set("deps:<request_id>", …)`). Self-shows
       when `depsExplorerStore.isOpen` flips to true. -->
  <DepsExplorerModal />

  <!-- Command Palette (Ctrl+K) -->
  {#if uiStore.commandPaletteOpen}
    <CommandPalette onClose={() => uiStore.setCommandPaletteOpen(false)} />
  {/if}

  <!-- Global Git Blame modal (dispatched via `arbor:show-blame`). Mounted
       here so any surface — Command Palette, plugins, context menus — can
       open it without depending on the File Tree sidebar being visible. -->
  {#if blameTarget}
    <GitBlameModal
      tabId={blameTarget.tabId}
      path={blameTarget.path}
      onClose={() => blameTarget = null}
    />
  {/if}

  <!-- Global Worktree Info modal (dispatched via `arbor:show-worktree-info`).
       Same rationale as GitBlameModal: the Command Palette can open it
       without the Worktrees sidebar section being expanded. -->
  {#if worktreeInfoTarget}
    {@const wt = worktreeInfoTarget}
    <WorktreeInfoModal
      worktree={wt}
      onClose={() => worktreeInfoTarget = null}
      onSwitch={() => { switchToWorktree(wt); worktreeInfoTarget = null; }}
      onOpenInIde={async (ideId) => {
        try { await openInIde(wt.path, ideId); }
        catch (e) { uiStore.showToast(`Failed to open IDE: ${e}`, 'error'); }
      }}
    />
  {/if}

  <!-- Singleton tooltip host: every `use:tooltip` action publishes through
       `tooltipState` and this renders the result. -->
  <Tooltip />

  <!-- Conflict resolution modal (merge & stash) -->
  {#if uiStore.mergeModalOpen}
    <ConflictResolutionModal mode="merge" />
  {/if}
  {#if uiStore.stashConflictModalOpen}
    <ConflictResolutionModal mode="stash" />
  {/if}
  {#if uiStore.checkoutConflictModalOpen}
    <CheckoutConflictModal />
  {/if}

  <!-- Init Repository modal — shown when opening a non-git folder -->
  {#if initModalOpen}
    <InitRepoModal
      path={initModalPath}
      onInit={handleInitRepo}
      onCancel={() => initModalOpen = false}
    />
  {/if}

  <!-- Clone Repository modal -->
  {#if cloneModalOpen}
    <CloneRepoModal
      onClose={() => cloneModalOpen = false}
      onCloned={() => cloneModalOpen = false}
    />
  {/if}

  <!-- Open Repository file picker -->
  {#if openPickerOpen}
    <FilePickerModal
      mode="folder"
      title="Open Repository"
      initialPath={uiStore.recentRepos[0]?.replace(/[\\/][^\\/]+$/, '') || undefined}
      onConfirm={handlePickerConfirm}
      onCancel={() => openPickerOpen = false}
    />
  {/if}

  <!-- Repository Browser Modal -->
  {#if uiStore.repoBrowserOpen}
    <RepoBrowserModal
      onClose={() => { uiStore.closeRepoBrowser(); repoBrowserStore.reset(); }}
    />
  {/if}

  <!-- Cloud-storage chunk-order picker. Self-mounts on the Tauri event
       `arbor://cloud-chunk-order-open`. Bulk-transfer progress is no longer
       a separate floater — it surfaces through the standard JobsOverlay
       (each download_many / sync is a JobInfo). -->
  <CloudChunkOrderModal />

  <!-- Stats Overlay — self-contained (uses shared Modal shell). -->
  {#if statsOverlayOpen}
    <StatsOverlay onClose={() => statsOverlayOpen = false} />
  {/if}

  <!-- Theme Editor Modal -->
  {#if themeEditorOpen}
    <ThemeEditorModal onClose={() => themeEditorOpen = false} />
  {/if}

  <!-- Bottom-right unified stack — top to bottom:
       1. Linked-worktree sync summary (when active — sticky at top until dismissed)
       2. Toasts + notifications, interleaved chronologically (oldest first,
          newest just above the operations zone)
       3. Operation cards — always anchored at the very bottom (above the
          status bar), so live progress stays in a stable place while toasts
          and notifications scroll up over time.
       Single fixed-positioned column avoids the cross-overlay overlap we
       used to fight by chasing z-index values. -->
  <div class="bottom-right-stack" aria-live="polite" aria-atomic="false">
    <WorktreeLinkSyncSummary />
    {#each feedItems as item (item.key)}
      {#if item.kind === 'toast'}
        <ToastItem toast={item.value} />
      {:else}
        <NotificationItem notif={item.value} alwaysShowDismiss />
      {/if}
    {/each}
    <OperationsOverlay />
  </div>

  <!-- Lazy-loaded — SettingsPanel and DocsPanel modules stay out of the
       initial JS heap until the user opens them. The dev warmup in onMount
       fires the same loaders eagerly so local development feels instant. -->
  <Lazy
    gate={showSettings}
    loader={() => import('./SettingsPanel.svelte')}
    onClose={() => uiStore.setPanel('graph')}
    onOpenThemeEditor={() => { uiStore.setPanel('graph'); themeEditorOpen = true; }}
  />
  {#if showPlugins}
    <PluginPanel onClose={() => uiStore.setPanel('graph')} />
  {/if}
  <Lazy
    gate={showDocs}
    loader={() => import('../shared/DocsPanel.svelte')}
    onClose={() => uiStore.setPanel('graph')}
  />

  <!-- Modal: Workspace Manager.
       onCreate keeps the Manager open — Create stacks on top so closing it
       returns the user exactly where they were (repo list, search position
       and all). -->
  {#if workspaceManagerOpen}
    <WorkspaceManagementModal
      onClose={() => workspaceManagerOpen = false}
      onCreate={() => createWorkspaceOpen = true}
    />
  {/if}

  <!-- Modal: Create / Edit Workspace -->
  {#if createWorkspaceOpen}
    <CreateWorkspaceModal
      onClose={() => createWorkspaceOpen = false}
    />
  {/if}

  <!-- Studio modals — lazy-imported on first open via their respective
       stores' `open` flag. Each module is large (rich tree/diff/inspector
       panes); keeping them off the initial heap is the biggest single win
       in the Tier-1 RAM plan. Rendered before PluginFormModal so a plugin
       form opened on top (e.g. an export config) wins z-order. -->
  <Lazy gate={jsonStudioStore.open}       loader={() => import('$lib/components/shared/JsonStudioModal.svelte')} />
  <Lazy gate={ronStudioStore.open}        loader={() => import('$lib/components/shared/RonStudioModal.svelte')} />
  <Lazy gate={tomlStudioStore.open}       loader={() => import('$lib/components/shared/TomlStudioModal.svelte')} />
  <Lazy gate={yamlStudioStore.open}       loader={() => import('$lib/components/shared/YamlStudioModal.svelte')} />
  <Lazy gate={propertiesStudioStore.open} loader={() => import('$lib/components/shared/PropertiesStudioModal.svelte')} />

  <!-- Modal: Plugin Form — rendered LAST among the plugin-trigger modals so
       it paints on top. Plugin actions fired from inside another modal
       (Plugin Manager, Workspace Manager row contributions, …) need to be
       visible to the user; otherwise the form opens behind the source modal
       and the user sees nothing happen. -->
  {#if pendingPluginForm}
    <!-- #key forces full remount when the form config changes (e.g. action → new form).
         After the first load the module is cached in V8's module map, so the
         dynamic import resolves in the same microtask and the swap is seamless. -->
    {#key pluginStore.formKey}
      <Lazy
        loader={() => import('../plugins/PluginFormModal.svelte')}
        form={pendingPluginForm}
        onClose={() => pluginStore.clearPendingForm()}
      />
    {/key}
  {/if}

  <!-- Plugin-requested file picker (arbor.ui.pick_file) — rendered AFTER
       PluginFormModal so a plugin that opens a picker FROM INSIDE its own
       form (e.g. source-export's "Import profile" button) sees the picker
       paint on top of the form. Without this ordering, the picker disappears
       behind the modal that triggered it (visible on Windows/WebView2). -->
  {#if pluginPickFile}
    {@const req = pluginPickFile}
    <FilePickerModal
      mode={req.mode ?? 'file'}
      title={req.title ?? 'Select a file'}
      extensions={req.extensions}
      initialPath={req.initial_path}
      onConfirm={(path) => {
        const ctx = { path, ...(req.extra ?? {}) };
        firePluginAction(req.plugin_name, req.action, JSON.stringify(ctx)).catch(() => {});
        pluginPickFile = null;
      }}
      onCancel={() => {
        // Fire with empty path so the plugin can distinguish "cancelled" from
        // "never opened" without needing a separate cancel action.
        const ctx = { path: '', ...(req.extra ?? {}) };
        firePluginAction(req.plugin_name, req.action, JSON.stringify(ctx)).catch(() => {});
        pluginPickFile = null;
      }}
    />
  {/if}

  <!-- Pipeline run deep-link modal (opens when pipelinesStore.activeRunId
       is set — by arbor.ui.show_pipeline_run or any other store writer).
       Rendered AFTER PluginFormModal so it paints on top: plugins commonly
       deep-link to a run from inside their own form (e.g. source-export's
       sequence detail), and the user expects the run modal to land above
       the form, not behind it. -->
  <PipelineRunDetailModal />

  <!-- Modal: ContributableModal — opened by arbor.ui.container.open() (or its
       sugar arbor.ui.settings.open()). Stacks above PluginFormModal because
       it paints later in source order (settings can be invoked from inside
       a form). -->
  {#if containerStore.openContainerId}
    {@const cid = containerStore.openContainerId}
    <Lazy
      loader={() => import('../plugins/ContributableModal.svelte')}
      containerId={cid}
      onClose={() => containerStore.close()}
    />
  {/if}

  <!-- Modal: Plugin Marketplace — mounted globally so the open_marketplace
       shortcut and the Command Palette can reach it without having to open
       the Plugin Manager first. Lazy-loaded: drags in the registry catalog,
       install-confirm and a heap of icons that aren't needed at startup. -->
  <Lazy
    gate={uiStore.marketplaceOpen}
    loader={() => import('../plugins/MarketplaceModal.svelte')}
    onClose={() => uiStore.closeMarketplace()}
  />

  <!-- Modal: Linked Worktrees (cross-project sync) -->
  <WorktreeLinkManagerModal />
  <AddToWorktreeLinkModal />
  <!-- WorktreeLinkSyncSummary is rendered inside .bottom-right-stack above so
       it stacks with toasts instead of overlapping them. -->

  <!-- Modal: About — self-contained (uses shared Modal shell). Lazy-loaded. -->
  <Lazy
    gate={showAbout}
    loader={() => import('../shared/AboutModal.svelte')}
    onClose={() => uiStore.setPanel('graph')}
  />

  <!-- Git Setup modal — blocking overlay when no `git` binary is configured.
       Stays mounted across the missing → downloading transition so the progress
       UI can stream.  Skipped during the initial `detecting` flash to avoid a
       100ms flicker on launches where git is found instantly. -->
  {#if !gitCliStore.status?.path && (gitCliStore.phase === 'missing' || gitCliStore.phase === 'downloading') && !gitBouncerDismissed}
    <GitSetupModal
      dismissable
      onClose={() => (gitBouncerDismissed = true)}
    />
  {/if}

  <!-- Welcome / onboarding tour. Auto-opens once per CURRENT_ONBOARDING_VERSION
       (see effect above); re-entry from the command palette / Docs link
       dispatches the `arbor:open-onboarding` window event. -->
  {#if onboardingStore.open}
    <OnboardingModal />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    background: var(--bg-base);
    overflow: hidden;
  }

  .content-area {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
  }

  /* ── Workspace: flex row containing ActivityBar + panels container.
     The workspace background is the "gap" colour revealed between panels,
     giving the IntelliJ-style soft-rounded inset look.  ActivityBar sits
     flush against the left edge; the inset starts at `.panels`. */
  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-elevated);
  }

  /* Inset container that holds the sidebar + main column.
     A small 4px gap between panels reveals the workspace bg colour and
     keeps the rounded-corner "stacco" visible between sidebar and editor.
     No top padding: the titlebar flows directly into the panels. */
  .panels {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    gap: 4px;
    padding: 0 4px 4px 4px;
  }

  .main-col {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    gap: 4px;
    background: transparent;
  }

  /* Editor card: wraps TabBar + graph + bisect banner as one rounded unit. */
  .editor-col {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  .graph-area {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  /* ── Panel layout wrappers (transitions applied inline via Svelte transition:) ── */
  .sidebar-wrap {
    display: flex;
    height: 100%;
    flex-shrink: 0;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }
  /* Mirror of .sidebar-wrap on the right of .panels. Background + radius
     match so it reads as a floating card just like the left one. */
  .right-sidebar-wrap {
    display: flex;
    height: 100%;
    flex-shrink: 0;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  .bottom-wrap {
    display: flex;
    flex-direction: column;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }

  .bottom-right-stack {
    position: fixed;
    bottom: 36px;
    right: 16px;
    z-index: 800;
    display: flex;
    flex-direction: column; /* sync summary on top, toasts below; new toasts append at the bottom */
    align-items: flex-end;
    gap: 8px;
    pointer-events: none;
    max-width: calc(100vw - 32px);
  }
  .bottom-right-stack > :global(*) { pointer-events: auto; }

</style>
