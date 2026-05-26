<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { animStore } from '$lib/stores/animations.svelte';
  // Direct imports cover only icons referenced as `<Foo />` literals in this
  // file's markup (no dynamic name → component lookup). Anything used via
  // string lookup (verb icons, plugin-supplied command icons, action items
  // built in code) goes through the shared PLUGIN_ICONS registry — no
  // duplicate per-component map allowed (PLUGIN_ICONS is the single source).
  import { Zap, Search, ChevronRight } from 'lucide-svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { invoke } from '@tauri-apps/api/core';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { worktreeStore } from '$lib/stores/worktree.svelte';
  import { mrStore } from '$lib/stores/mr.svelte';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { setWorktreeLinkSyncEnabled, removeWorktreeLinkMember } from '$lib/ipc/linkedWorktree';
  import type { WorkspaceDef, RepoRegistryEntry } from '$lib/types/workspace';
  import { activityBarConfigStore } from '$lib/stores/activityBarConfig.svelte';
  import { firePluginAction, reloadPlugins } from '$lib/ipc/plugin';
  import {
    checkoutBranch, checkoutBranchSafe, mergeBranch, deleteBranch, createBranch,
    stashApply, stashPop, stashDrop, resetToCommit,
    listStashes,
  } from '$lib/ipc/branch';
  import { handleCheckoutResult } from '$lib/utils/checkoutResultHandler';
  import type { MergeStrategy } from '$lib/ipc/branch';
  import { pushBranch, fetchRemote, pullBranch, listRemotes } from '$lib/ipc/remote';
  import { handlePullResult, handlePullThrown } from '$lib/utils/pullResultHandler';
  import { applyPostStashChange } from '$lib/utils/applyPostStashChange';
  import { applyPostCheckout } from '$lib/utils/applyPostCheckout';
  import { startPullOperation } from '$lib/utils/operations-bridge';
  import { cherryPick, revertCommit, stageAll, unstageAll, discardAll } from '$lib/ipc/stage';
  import { updateAllSubmodules } from '$lib/ipc/submodule';
  import { getCommitDetail, openRepo as ipcOpenRepo, getGraph, getRepoFiles } from '$lib/ipc/graph';
  import { getStatus } from '$lib/ipc/stage';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { openInIde, listWorktrees } from '$lib/ipc/worktree';
  import { switchToWorktree } from '$lib/utils/worktree-switch';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import type {
    BranchInfo, SearchResult, RepoStatus, TagInfo, StashEntry, RemoteInfo,
    WorktreeInfo,
  } from '$lib/types/git';
  import type { PluginCommand } from '$lib/types/plugin';
  import type { RepoTab } from '$lib/stores/tabs.svelte';
  import type { MergeRequest } from '$lib/types/mr';
  import type { Theme } from '$lib/types/theme';
  import type { Issue } from '$lib/types/issues';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import {
    linearGetAuthStatus, jiraGetAuthStatus,
    linearSearchIssues, jiraSearchIssues,
  } from '$lib/ipc/issues';
  import { getBranchPolicy, assertBranchNameAllowed, type BranchPolicy } from '$lib/utils/branch-policy';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { shortcutFor } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  // Confirm-prompt state — replaces window.confirm. While set, ConfirmModal
  // is rendered on top of the palette; on confirm we run the supplied
  // callback (which is responsible for calling onClose), on cancel we just
  // close the palette like the old prompt-cancel behaviour.
  type ConfirmReq = {
    title:        string;
    message:      string;
    detail?:      string;
    variant?:     'default' | 'danger' | 'warning' | 'info';
    confirmLabel?: string;
    onConfirm:    () => void | Promise<void>;
  };
  let pendingConfirm = $state<ConfirmReq | null>(null);
  function askConfirm(req: ConfirmReq) { pendingConfirm = req; }

  // ── Types ────────────────────────────────────────────────────────────────────

  type PaletteItemKind =
    | 'action' | 'branch' | 'commit' | 'project-file' | 'plugin' | 'verb'
    | 'stash'  | 'tag'    | 'remote' | 'tab'  | 'recent' | 'mr' | 'theme';

  interface PaletteItem {
    id:        string;
    kind:      PaletteItemKind;
    title:     string;
    subtitle?: string;
    icon:      string;        // lucide component name key
    shortcut?: string;
    score:     number;
    action: () => void | Promise<void>;
  }

  interface PaletteSection {
    id:    string;
    label: string;
    items: PaletteItem[];
  }

  // Icon resolver — delegates to the shared PLUGIN_ICONS registry so a
  // plugin's `icon = "EyeOff"` here resolves to the same component as
  // anywhere else in the app. Falls back to Zap when the name is unknown.


  // ── State ────────────────────────────────────────────────────────────────────

  let query        = $state('');
  let selectedIdx  = $state(0);
  let inputEl      = $state<HTMLInputElement | undefined>(undefined);

  // Verb chip — when set, the palette is in Phase 2 (target picker).
  let selectedVerb = $state<VerbDef | null>(null);

  // Create-branch verb has two sub-steps: first the user types the branch
  // name, then (optionally, via Tab) picks a parent commit other than HEAD.
  // `pendingBranchName` survives the step transition so the input can be
  // reused for filtering commits.
  let createBranchStep  = $state<'name' | 'parent'>('name');
  let pendingBranchName = $state('');
  // Resolved per-tab on mount. Drives the ticket-suggestions UX in step='name'
  // and gates branch creation through `assertBranchNameAllowed`.
  let branchPolicy      = $state<BranchPolicy>({ requireTicket: false, tracker: null, ticketRegex: null });

  // Async data loaded on open
  let branches    = $state<BranchInfo[]>([]);
  let tags        = $state<TagInfo[]>([]);
  let commits     = $state<SearchResult[]>([]);
  let status      = $state<RepoStatus | null>(null);
  let stashes     = $state<StashEntry[]>([]);
  let remotes     = $state<RemoteInfo[]>([]);
  /// All tracked + untracked paths in the active repo. Populated lazily the
  /// first time the user enters a verb whose `targetKind === 'project-file'`,
  /// because the call (`getRepoFiles` → libgit2 index scan) is cheap but not
  /// needed for the common branch/commit/stash flows.
  let projectFiles = $state<string[]>([]);
  let projectFilesLoaded = $state(false);
  /// Worktree list for the active tab. Loaded the first time a verb whose
  /// `targetKind === 'worktree'` becomes active — same lazy pattern as
  /// project files.
  let worktrees    = $state<WorktreeInfo[]>([]);
  let worktreesLoaded = $state(false);
  let loading     = $state(false);

  // Debounce timer for commit search
  let commitDebounce: ReturnType<typeof setTimeout> | undefined;

  // ── Cross-tab issue query (Linear / Jira) ──────────────────────────────────
  // Independent of the active tab's tracker config — the verbs are visible
  // whenever the user is authenticated to that provider, and search runs
  // straight against the provider IPC bypassing `issuesStore` so per-repo
  // default project filters don't apply (per UX request: no project filter
  // at the palette level). The placeholder hint surfaces the same #/~
  // prefixes that the sidebar's search box supports — the backend
  // interprets them, the palette just passes the query through.
  let linearAuthed   = $state(false);
  let jiraAuthed     = $state(false);
  let linearIssues   = $state<Issue[]>([]);
  let jiraIssues     = $state<Issue[]>([]);
  let linearLoading  = $state(false);
  let jiraLoading    = $state(false);
  let issueDebounce: ReturnType<typeof setTimeout> | undefined;

  // Guard against re-entrancy in the query effect when we auto-promote a verb.
  let autoPromoting = false;

  function enterVerb(v: VerbDef, rest: string = '') {
    autoPromoting = true;
    selectedVerb = v;
    query = rest;
    selectedIdx = 0;
    commits = [];
    createBranchStep = 'name';
    pendingBranchName = '';
    // Kick off the target-kind data load here — the post-mount $effect that
    // also calls these is gated by `autoPromoting` and runs a tick later,
    // which is why the dropdown used to stay empty until the user typed the
    // first character. Calling directly means the IPC fires the moment the
    // verb is selected. The async helpers are idempotent (no-op when the
    // data is already cached), so the effect can keep them as a safety net
    // without duplicating work.
    if (v.targetKind === 'commit' && v.id !== 'create-branch') {
      // Seed from the already-loaded graph so the dropdown is populated
      // synchronously; the debounced `loadCommits` overrides this once the
      // user has typed ≥ 2 characters.
      seedCommitsFromGraph();
    }
    if (v.targetKind === 'worktree')     void ensureWorktrees();
    if (v.targetKind === 'project-file') void ensureProjectFiles();
    if (v.targetKind === 'mr')           void ensureMrList();
    if (v.targetKind === 'linear-issue' || v.targetKind === 'jira-issue') {
      // Kick off an initial empty-query search so the list is populated the
      // moment the user enters the verb chip; subsequent keystrokes go
      // through the debounced effect below.
      void runIssueSearch(v.targetKind, rest);
    }
    tick().then(() => {
      inputEl?.focus();
      if (inputEl) inputEl.selectionStart = inputEl.selectionEnd = query.length;
      autoPromoting = false;
    });
  }

  function clearVerb() {
    selectedVerb = null;
    selectedIdx = 0;
    query = '';
    commits = [];
    createBranchStep = 'name';
    pendingBranchName = '';
    tick().then(() => inputEl?.focus());
  }

  // ── Built-in leaf actions ───────────────────────────────────────────────────
  //
  // Derived rather than const so items can toggle based on repo state
  // (e.g. "Continue Rebase" only shows while a rebase is in progress).

  type LeafAction = Omit<PaletteItem, 'score'> & {
    /** Optional group label used to organise Phase 1 actions into sub-sections. */
    group?: string;
  };

  // Sidebar section descriptors for "Show …" actions (declared before
  // `buildBuiltinActions` uses it to avoid a temporal-dead-zone error).
  const SIDEBAR_SECTIONS: { id: string; title: string; icon: string }[] = [
    { id: 'branches', title: 'Show Branches & Stashes', icon: 'GitBranch' },
    { id: 'gitflow',  title: 'Show Git Flow',           icon: 'GitMerge'  },
    { id: 'mr',       title: 'Show Pull / Merge Requests', icon: 'GitPullRequest' },
    { id: 'issues',   title: 'Show Issues',             icon: 'Sparkles'  },
    { id: 'files',    title: 'Show Files',              icon: 'FolderGit2' },
    { id: 'reflog',   title: 'Show Reflog',             icon: 'History'   },
    { id: 'stats',    title: 'Show Repository Stats',   icon: 'BarChart2' },
    { id: 'security', title: 'Show Security Dashboard', icon: 'ShieldAlert' },
  ];

  // Maps a palette leaf-action id to the matching built-in keybinding action
  // id (in keybindings.ts). The palette renders the live shortcut next to
  // the row so users discover the kbd as they hunt for the command — and a
  // user remap in Settings → Keybindings flows here automatically through
  // `shortcutFor`. Items without an entry here intentionally have no kbd.
  const LEAF_TO_KEYBINDING: Record<string, string> = {
    'action:open-repo':          'open_repo',
    'action:clone-repo':         'clone_repo',
    'action:init-repo':          'init_repo',
    'action:close-tab':          'close_tab',
    'action:next-tab':           'next_tab',
    'action:prev-tab':           'prev_tab',
    'action:pull':               'pull',
    'action:push':               'push',
    'action:fetch':              'fetch',
    'action:stash':              'stash',
    'action:commit':             'commit',
    'action:stage-all':          'stage_all',
    'action:unstage-all':        'unstage_all',
    'action:manage-workspaces':  'workspace_manager',
    'action:terminal':           'toggle_terminal',
    'action:sidebar':            'toggle_sidebar',
    'action:stage':              'stage_view',
    'action:settings':           'settings',
    'action:plugins':            'plugins',
    'action:marketplace':        'open_marketplace',
    'action:docs':               'toggle_docs',
    'action:jump-head':          'jump_to_head',
    // Sidebar / bottom section toggles — auto-generated `action:show-<id>` rows
    'action:show-branches':      'toggle_branches_sidebar',
    'action:show-files':         'toggle_files_sidebar',
    'action:show-gitflow':       'toggle_gitflow_sidebar',
    'action:show-issues':        'toggle_issues_sidebar',
    'action:show-mr':            'toggle_mr_sidebar',
    'action:show-reflog':        'toggle_reflog_sidebar',
    'action:show-stats':         'toggle_stats_sidebar',
    'action:show-security':      'toggle_security_sidebar',
    'action:show-pipelines':     'toggle_pipelines_panel',
  };

  const builtinActions = $derived<LeafAction[]>(decorateWithShortcuts(buildBuiltinActions()));

  /** Resolve a per-action shortcut once at build time so it tracks keybindings live. */
  function decorateWithShortcuts(items: LeafAction[]): LeafAction[] {
    return items.map(item => {
      if (item.shortcut) return item;
      const kb = LEAF_TO_KEYBINDING[item.id];
      if (!kb) return item;
      const sc = shortcutFor(kb);
      return sc ? { ...item, shortcut: sc } : item;
    });
  }

  function buildBuiltinActions(): LeafAction[] {
    const hasTab       = tabsStore.activeTab !== null;
    const isRebasing   = status?.is_rebasing ?? false;
    const isMerging    = status?.is_merging  ?? false;
    const hasStaged    = (status?.staged.length ?? 0) > 0;
    const hasUnstaged  = ((status?.unstaged.length ?? 0) + (status?.untracked.length ?? 0)) > 0;
    const headBranch   = branches.find(b => b.is_head) ?? null;
    const currentSha   = headBranch?.head_oid ?? null;

    const actions: LeafAction[] = [];

    // ── Repository ──────────────────────────────────────────────────────────
    actions.push(
      { id: 'action:open-repo',     kind: 'action', icon: 'FolderOpen',  group: 'Repository',
        title: 'Open Repository',   subtitle: 'Pick a folder',
        action: () => closeAndDispatch('arbor:open-repo') },
      { id: 'action:init-repo',     kind: 'action', icon: 'FolderPlus',  group: 'Repository',
        title: 'Init Repository',   subtitle: 'Pick a folder; a non-git one is initialised',
        action: () => closeAndDispatch('arbor:init-repo') },
      { id: 'action:clone-repo',    kind: 'action', icon: 'Download',    group: 'Repository',
        title: 'Clone Repository',  subtitle: 'Clone a remote repo into a new tab',
        action: () => closeAndDispatch('arbor:clone-repo') },
    );
    if (hasTab) {
      actions.push(
        { id: 'action:reload-repo', kind: 'action', icon: 'RefreshCw',   group: 'Repository',
          title: 'Reload Repository', subtitle: 'Re-read git state for the current tab',
          action: () => closeAndDispatch('arbor:reload-repo') },
      );
    }

    // ── Workspaces ──────────────────────────────────────────────────────────
    actions.push(
      { id: 'action:manage-workspaces', kind: 'action', icon: 'Layers', group: 'Workspaces',
        title: 'Manage Workspaces', subtitle: 'Open the workspace manager',
        action: () => closeAndDispatch('arbor:manage-workspaces') },
      { id: 'action:create-workspace', kind: 'action', icon: 'Plus', group: 'Workspaces',
        title: 'Create Workspace', subtitle: 'Create a new workspace',
        action: () => closeAndDispatch('arbor:create-workspace') },
    );

    // ── Tabs ────────────────────────────────────────────────────────────────
    if (hasTab) {
      actions.push(
        { id: 'action:close-tab',   kind: 'action', icon: 'X',            group: 'Tabs',
          title: 'Close Current Tab',
          action: () => {
            const t = tabsStore.activeTab;
            if (t) tabsStore.removeTab(t.id);
            onClose();
          } },
      );
      if (tabsStore.tabs.length > 1) {
        actions.push(
          { id: 'action:next-tab',  kind: 'action', icon: 'ArrowLeftRight', group: 'Tabs',
            title: 'Next Tab',
            action: () => { tabsStore.nextTab(); onClose(); } },
          { id: 'action:prev-tab',  kind: 'action', icon: 'ArrowLeftRight', group: 'Tabs',
            title: 'Previous Tab',
            action: () => { tabsStore.prevTab(); onClose(); } },
        );
      }
    }

    // ── Linked Worktrees (cross-project sync) ──────────────────────────────
    actions.push(
      { id: 'action:links-manage', kind: 'action', icon: 'Layers', group: 'Linked Worktrees',
        title: 'Manage Linked Worktrees', subtitle: 'Configure cross-project worktree sync',
        action: () => { uiStore.openLinkManager(); onClose(); } },
    );
    if (hasTab) {
      const t = tabsStore.activeTab;
      const repoId = t ? (workspacesStore.registry.find(r => r.path === t.path)?.id ?? null) : null;
      if (repoId) {
        const link = linkedWorktreesStore.linkForRepo(repoId);
        if (!link) {
          actions.push(
            { id: 'action:add-to-link', kind: 'action', icon: 'FolderPlus', group: 'Linked Worktrees',
              title: 'Link this Worktree…',
              action: () => { uiStore.openAddToLink(repoId); onClose(); } },
          );
        } else {
          actions.push(
            { id: 'action:remove-from-link', kind: 'action', icon: 'Trash2', group: 'Linked Worktrees',
              title: `Unlink from "${link.name}"`,
              action: () => {
                askConfirm({
                  title: 'Unlink worktree',
                  message: `Remove this worktree from "${link.name}"?`,
                  variant: 'danger',
                  confirmLabel: 'Unlink',
                  onConfirm: async () => {
                    try { await removeWorktreeLinkMember(link.id, repoId); }
                    catch (e) { uiStore.showToast(`${e}`, 'error'); }
                    onClose();
                  },
                });
              } },
            { id: 'action:toggle-link-sync', kind: 'action', icon: 'RefreshCw', group: 'Linked Worktrees',
              title: link.sync_enabled
                ? `Disable Sync for "${link.name}"`
                : `Enable Sync for "${link.name}"`,
              action: async () => {
                try { await setWorktreeLinkSyncEnabled(link.id, !link.sync_enabled); }
                catch (e) { uiStore.showToast(`${e}`, 'error'); }
                onClose();
              } },
          );
        }
      }
    }

    // ── Git: common ─────────────────────────────────────────────────────────
    if (hasTab) {
      actions.push(
        { id: 'action:pull',         kind: 'action', icon: 'Download',    group: 'Git',
          title: 'Pull',              subtitle: 'Pull current branch',
          action: () => closeAndDispatch('arbor:pull') },
        { id: 'action:push',         kind: 'action', icon: 'ArrowUpToLine', group: 'Git',
          title: 'Push',              subtitle: 'Push current branch',
          action: () => closeAndDispatch('arbor:push') },
        { id: 'action:fetch',        kind: 'action', icon: 'RefreshCw',   group: 'Git',
          title: 'Fetch All Remotes',
          action: () => closeAndDispatch('arbor:fetch') },
        { id: 'action:stash',        kind: 'action', icon: 'Layers',      group: 'Git',
          title: 'Stash Changes',
          action: () => closeAndDispatch('arbor:stash') },
      );
    }

    // ── Stage & Commit ──────────────────────────────────────────────────────
    if (hasTab) {
      actions.push(
        { id: 'action:commit',       kind: 'action', icon: 'GitCommit',   group: 'Stage & Commit',
          title: 'Commit',             subtitle: 'Focus the commit message textarea',
          action: () => focusCommitForm(false) },
        { id: 'action:amend',        kind: 'action', icon: 'Pencil',      group: 'Stage & Commit',
          title: 'Amend Last Commit', subtitle: 'Edit the last commit',
          action: () => focusCommitForm(true) },
      );
      if (hasUnstaged) {
        actions.push(
          { id: 'action:stage-all',  kind: 'action', icon: 'CheckSquare', group: 'Stage & Commit',
            title: 'Stage All Changes',
            action: () => runStageOp('stage') },
        );
      }
      if (hasStaged) {
        actions.push(
          { id: 'action:unstage-all', kind: 'action', icon: 'Eraser',     group: 'Stage & Commit',
            title: 'Unstage All Changes',
            action: () => runStageOp('unstage') },
        );
      }
      if (hasUnstaged) {
        actions.push(
          { id: 'action:discard-all', kind: 'action', icon: 'Trash2',     group: 'Stage & Commit',
            title: 'Discard All Changes', subtitle: 'Throw away unstaged changes',
            action: () => runStageOp('discard') },
        );
      }
      if (currentSha) {
        actions.push(
          { id: 'action:undo-commit', kind: 'action', icon: 'Undo2',      group: 'Stage & Commit',
            title: 'Undo Last Commit',  subtitle: 'Soft reset HEAD~1 (keeps changes staged)',
            action: () => undoLastCommit() },
        );
      }
    }

    // ── Rebase / Merge state ────────────────────────────────────────────────
    if (hasTab && isRebasing) {
      actions.push(
        { id: 'action:rebase-continue', kind: 'action', icon: 'ChevronsRight', group: 'Rebase',
          title: 'Continue Rebase',
          action: () => runRebase('continue') },
        { id: 'action:rebase-skip',     kind: 'action', icon: 'ChevronRight',  group: 'Rebase',
          title: 'Skip Rebase Step',
          action: () => runRebase('skip') },
        { id: 'action:rebase-abort',    kind: 'action', icon: 'StopCircle',    group: 'Rebase',
          title: 'Abort Rebase',
          action: () => runRebase('abort') },
      );
    }
    if (hasTab && isMerging) {
      actions.push(
        { id: 'action:merge-abort', kind: 'action', icon: 'StopCircle', group: 'Merge',
          title: 'Abort Merge',
          action: () => abortMergeAction() },
      );
    }

    // ── Merge Requests ──────────────────────────────────────────────────────
    // Hidden entirely when the active repo has MR/PRs disabled on the
    // provider (probed via probeMrFeature, cached on mrStore.mrFeature).
    if (hasTab && mrStore.mrFeature?.enabled !== false) {
      actions.push(
        { id: 'action:create-mr', kind: 'action', icon: 'GitPullRequest', group: 'Merge Requests',
          title: 'Open Pull / Merge Request',
          subtitle: 'Create a new merge / pull request',
          action: () => closeAndDispatch('arbor:create-mr') },
      );
    }

    // ── Panels ──────────────────────────────────────────────────────────────
    actions.push(
      { id: 'action:stage',        kind: 'action', icon: 'FileCode',       group: 'Panels',
        title: 'Toggle Stage Area',
        action: () => { uiStore.toggleBottomSection('stage'); onClose(); } },
      { id: 'action:detail',       kind: 'action', icon: 'PanelBottom',    group: 'Panels',
        title: 'Toggle Commit Detail',
        action: () => { uiStore.toggleBottomSection('detail'); onClose(); } },
      { id: 'action:terminal',     kind: 'action', icon: 'Terminal',       group: 'Panels',
        title: 'Toggle Terminal',
        action: () => { uiStore.toggleBottomSection('terminal'); onClose(); } },
      { id: 'action:jobs',         kind: 'action', icon: 'Play',           group: 'Panels',
        title: 'Toggle Jobs Overlay',
        action: () => { uiStore.toggleJobsOverlay(); onClose(); } },
      { id: 'action:notifications', kind: 'action', icon: 'Bell',          group: 'Panels',
        title: 'Toggle Notifications',
        action: () => { uiStore.toggleNotificationsOverlay(); onClose(); } },
      { id: 'action:sidebar',      kind: 'action', icon: 'PanelLeft',      group: 'Panels',
        title: 'Toggle Sidebar',
        action: () => { uiStore.toggleSidebarVisibility(); onClose(); } },
    );
    // Sidebar sections — respect user-configured visibility.
    for (const sec of SIDEBAR_SECTIONS) {
      if (!activityBarConfigStore.isVisible(sec.id)) continue;
      // Hide the MR section entry when MR/PRs are disabled for this repo.
      if (sec.id === 'mr' && mrStore.mrFeature?.enabled === false) continue;
      actions.push({
        id: `action:show-${sec.id}`, kind: 'action', icon: sec.icon, group: 'Panels',
        title: sec.title,
        action: () => { uiStore.setActiveSidebarSection(sec.id); onClose(); },
      });
    }
    if (activityBarConfigStore.isVisible('pipelines')) {
      actions.push({
        id: 'action:show-pipelines', kind: 'action', icon: 'Package', group: 'Panels',
        title: 'Show Pipelines Panel',
        action: () => { uiStore.setActiveBottomSection('pipelines'); onClose(); },
      });
    }

    // ── Copy ────────────────────────────────────────────────────────────────
    if (hasTab) {
      if (headBranch) {
        actions.push({
          id: 'action:copy-branch', kind: 'action', icon: 'Copy',  group: 'Copy',
          title: 'Copy Current Branch Name', subtitle: headBranch.name,
          action: () => copyText(headBranch.name, 'Branch name'),
        });
      }
      if (currentSha) {
        actions.push({
          id: 'action:copy-sha', kind: 'action', icon: 'Hash', group: 'Copy',
          title: 'Copy Current SHA', subtitle: currentSha.slice(0, 12),
          action: () => copyText(currentSha, 'Commit SHA'),
        });
      }
      const firstRemote = remotes[0];
      if (firstRemote) {
        actions.push({
          id: 'action:copy-remote-url', kind: 'action', icon: 'Globe', group: 'Copy',
          title: `Copy ${firstRemote.name} URL`, subtitle: firstRemote.url,
          action: () => copyText(firstRemote.url, 'Remote URL'),
        });
        // The repo-open deep link only needs a remote — no target picker.
        actions.push({
          id: 'action:copy-deep-link-repo', kind: 'action', icon: 'Link2', group: 'Copy',
          title: 'Copy arbor:// Link to Open Repository',
          subtitle: 'Shareable link that re-opens this repo on any machine',
          action: async () => {
            const tabId = tabsStore.activeTabId;
            if (tabId) await copyDeepLink({ kind: 'repo_open' }, tabId);
            onClose();
          },
        });
      }
    }

    // ── System ──────────────────────────────────────────────────────────────
    actions.push(
      { id: 'action:settings',     kind: 'action', icon: 'Settings',  group: 'System',
        title: 'Settings',
        action: () => { uiStore.setPanel('settings'); onClose(); } },
      { id: 'action:plugins',      kind: 'action', icon: 'Plug',      group: 'System',
        title: 'Plugin Manager',
        action: () => { uiStore.setPanel('plugins'); onClose(); } },
      { id: 'action:marketplace',  kind: 'action', icon: 'Store',     group: 'System',
        title: 'Plugin Marketplace', subtitle: 'Browse and install plugins & themes',
        action: () => { uiStore.openMarketplace(); onClose(); } },
      { id: 'action:reload-plugins', kind: 'action', icon: 'RefreshCw', group: 'System',
        title: 'Reload Plugins',
        action: () => reloadAllPlugins() },
      { id: 'action:docs',         kind: 'action', icon: 'FileText',  group: 'System',
        title: 'Documentation',
        action: () => { uiStore.setPanel('docs'); onClose(); } },
      { id: 'action:welcome-tour', kind: 'action', icon: 'Sparkles',  group: 'System',
        title: 'Welcome Tour',
        subtitle: 'Re-open the first-run onboarding walkthrough',
        action: () => closeAndDispatch('arbor:open-onboarding') },
      { id: 'action:about',        kind: 'action', icon: 'Info',      group: 'System',
        title: 'About Arbor',
        action: () => { uiStore.setPanel('about'); onClose(); } },
    );

    // ── Misc (repo-level) ───────────────────────────────────────────────────
    if (hasTab) {
      actions.push(
        { id: 'action:update-submodules', kind: 'action', icon: 'Package', group: 'Submodules',
          title: 'Update All Submodules',
          action: () => updateSubmodulesAction() },
      );
    }

    // ── Navigation ──────────────────────────────────────────────────────────
    if (hasTab) {
      actions.push(
        { id: 'action:jump-head', kind: 'action', icon: 'Star', group: 'Navigation',
          title: 'Jump to HEAD',
          action: () => { window.dispatchEvent(new CustomEvent('arbor:jump-to-head')); onClose(); } },
        { id: 'action:open-ide',  kind: 'action', icon: 'ExternalLink', group: 'Navigation',
          title: 'Open in IDE', subtitle: 'Open current workspace in the default IDE',
          action: () => openCurrentWorkspace() },
      );
    }

    return actions;
  }

  // ── Action helpers ─────────────────────────────────────────────────────────

  async function copyText(text: string, label: string) {
    await copyToClipboard(text, {
      successToast: `${label} copied`,
      errorToast: `Failed to copy ${label.toLowerCase()}`,
    });
    onClose();
  }

  async function runStageOp(op: 'stage' | 'unstage' | 'discard') {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return; }
    try {
      if (op === 'stage')   { await stageAll(tabId);   uiStore.showToast('Staged all changes',   'success'); }
      if (op === 'unstage') { await unstageAll(tabId); uiStore.showToast('Unstaged all changes', 'success'); }
      if (op === 'discard') {
        askConfirm({
          title: 'Discard all changes',
          message: 'Discard ALL unstaged changes?',
          detail: 'This cannot be undone.',
          variant: 'danger',
          confirmLabel: 'Discard',
          onConfirm: async () => {
            try {
              await discardAll(tabId);
              uiStore.showToast('Discarded all unstaged changes', 'success');
            } catch (e) {
              uiStore.showToast(`discard failed: ${e}`, 'error');
            }
            onClose();
          },
        });
        return;
      }
    } catch (e) {
      uiStore.showToast(`${op} failed: ${e}`, 'error');
    }
    onClose();
  }

  /** Ensure the stage panel is open, then ask CommitForm to focus its textarea. */
  function focusCommitForm(amend: boolean) {
    if (uiStore.activeBottomSection !== 'stage') uiStore.setActiveBottomSection('stage');
    // Defer so CommitForm is mounted and its event listener is registered.
    setTimeout(() => {
      window.dispatchEvent(new CustomEvent('arbor:focus-commit-form', { detail: { amend } }));
    }, 60);
    onClose();
  }

  async function undoLastCommit() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return; }
    const head = branches.find(b => b.is_head);
    if (!head) { uiStore.showToast('No HEAD branch detected', 'warning'); onClose(); return; }
    try {
      const detail = await getCommitDetail(tabId, head.head_oid);
      const parent = detail.parent_oids[0];
      if (!parent) { uiStore.showToast('HEAD has no parent — cannot undo', 'warning'); onClose(); return; }
      askConfirm({
        title: 'Undo last commit',
        message: `Soft-reset HEAD to ${parent.slice(0, 7)}?`,
        detail: 'Changes stay in the index.',
        variant: 'warning',
        confirmLabel: 'Undo commit',
        onConfirm: async () => {
          try {
            await resetToCommit(tabId, parent, 'soft');
            uiStore.showToast('Last commit undone (soft reset)', 'success');
            await refreshAfterReset(tabId);
          } catch (e) {
            uiStore.showToast(`Undo failed: ${e}`, 'error');
          }
          onClose();
        },
      });
      return;
    } catch (e) {
      uiStore.showToast(`Undo failed: ${e}`, 'error');
    }
    onClose();
  }

  async function runRebase(op: 'continue' | 'abort' | 'skip') {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return; }
    try {
      await invoke<void>(`rebase_${op}`, { tabId });
      uiStore.showToast(`Rebase ${op}ed`, 'success');
    } catch (e) {
      uiStore.showToast(`Rebase ${op} failed: ${e}`, 'error');
    }
    onClose();
  }

  /**
   * Shared driver for the four merge-strategy palette commands. Mirrors the
   * outcome-aware toast wording from BranchTree's drag-and-drop drop menu so
   * the two entry points stay in sync.
   */
  async function runMergeStrategy(
    tabId: string,
    source: BranchInfo,
    strategy: MergeStrategy,
    label: string,
  ) {
    if (source.is_head) {
      uiStore.showToast("Can't merge a branch into itself", 'warning');
      onClose();
      return;
    }
    try {
      const outcome = await mergeBranch(tabId, source.name, strategy);
      switch (outcome) {
        case 'already_up_to_date':
          uiStore.showToast(`HEAD already contains "${source.name}" — nothing to merge`, 'info');
          break;
        case 'fast_forward':
          uiStore.showToast(`Fast-forwarded HEAD to "${source.name}"`, 'success');
          break;
        case 'merged':
          uiStore.showToast(`${label}: merged "${source.name}" into HEAD`, 'success');
          break;
        case 'squashed':
          uiStore.showToast(
            `Squashed "${source.name}" into the index — review and commit from Stage`,
            'success',
          );
          break;
      }
      graphStore.refresh();
    } catch (e) {
      const msg = String(e);
      if (msg.toUpperCase().startsWith('CONFLICTS')) {
        uiStore.showToast('Merge produced conflicts — resolve in Stage', 'warning');
        graphStore.refresh();
      } else if (strategy === 'ff_only' && msg.toLowerCase().includes('not possible')) {
        uiStore.showToast(`Fast-forward not possible for "${source.name}"`, 'warning');
      } else {
        uiStore.showToast(`Merge failed: ${msg}`, 'error');
      }
    }
    onClose();
  }

  async function abortMergeAction() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return; }
    try {
      await invoke<void>('abort_merge', { tabId });
      uiStore.showToast('Merge aborted', 'success');
    } catch (e) {
      uiStore.showToast(`Abort merge failed: ${e}`, 'error');
    }
    onClose();
  }

  async function updateSubmodulesAction() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return; }
    try {
      await updateAllSubmodules(tabId, false);
      uiStore.showToast('Submodules updated', 'success');
    } catch (e) {
      uiStore.showToast(`Submodule update failed: ${e}`, 'error');
    }
    onClose();
  }

  async function reloadAllPlugins() {
    try {
      await reloadPlugins();
      uiStore.showToast('Plugins reloaded', 'success');
    } catch (e) {
      uiStore.showToast(`Plugin reload failed: ${e}`, 'error');
    }
    onClose();
  }

  // ── Open With: IDE launchers ────────────────────────────────────────────────

  /**
   * Built-in IDEs that have been detected (available on PATH or with an
   * explicit `path_overrides` entry) plus any user-defined custom IDEs.
   * Each entry becomes an "Open in <IDE>" item in the dedicated section.
   */
  const openWithIdes = $derived.by(() => {
    const detected = worktreeStore.detectedIdes
      .filter(d => d.available)
      .map(d => ({ id: d.id, name: d.name }));
    const customs = (worktreeStore.ideConfig?.custom_ides ?? [])
      .map(c => ({ id: c.id, name: c.name }));
    // Deduplicate by id — custom overrides detected (same id wins).
    const seen = new Set<string>();
    return [...customs, ...detected].filter(e => {
      if (seen.has(e.id)) return false;
      seen.add(e.id);
      return true;
    });
  });

  async function openCurrentWorkspace(ideId?: string) {
    const path = tabsStore.activeTab?.path;
    if (!path) {
      uiStore.showToast('No workspace is currently open', 'warning');
      onClose();
      return;
    }
    try {
      await openInIde(path, ideId);
      onClose();
    } catch (e) {
      uiStore.showToast(`Failed to launch IDE: ${e}`, 'error');
    }
  }

  // ── Scoring / fuzzy ─────────────────────────────────────────────────────────

  function score(text: string, q: string): number {
    if (!q) return 50;
    const t = text.toLowerCase();
    const lq = q.toLowerCase();
    if (t === lq)            return 100;
    if (t.startsWith(lq))   return 85;
    // word-boundary match
    const words = t.split(/[\s\-_\/\.]+/);
    if (words.some(w => w.startsWith(lq))) return 70;
    if (t.includes(lq))     return 55;
    // fuzzy: all chars of q appear in order
    let idx = 0;
    for (const ch of lq) {
      const found = t.indexOf(ch, idx);
      if (found === -1) return 0;
      idx = found + 1;
    }
    return 30;
  }

  function matches(text: string, q: string): boolean {
    return score(text, q) > 0;
  }

  // ── Data loading ────────────────────────────────────────────────────────────

  async function loadBranchesAndStatus() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    try {
      loading = true;
      const [b, t, s, st, rm] = await Promise.all([
        invoke<BranchInfo[]>('list_local_branches', { tabId }),
        invoke<TagInfo[]>('list_tags', { tabId }),
        invoke<RepoStatus>('get_status', { tabId }),
        listStashes(tabId).catch(() => [] as StashEntry[]),
        listRemotes(tabId).catch(() => [] as RemoteInfo[]),
      ]);
      branches = b;
      tags     = t;
      status   = s;
      stashes  = st;
      remotes  = rm;
    } catch { /* repo not open */ } finally {
      loading = false;
    }
  }

  /// Cheap one-shot fetch of the full project file list (tracked + untracked,
  /// served from the index). Cached for the lifetime of the palette open.
  async function ensureProjectFiles() {
    if (projectFilesLoaded) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    try {
      projectFiles = await getRepoFiles(tabId);
    } catch {
      projectFiles = [];
    }
    projectFilesLoaded = true;
  }

  /// Lazy MR-list fetch driven by entering an `mr` target-kind verb. Uses
  /// `mrStore.loadAll` (open + merged + closed in a single list) rather than
  /// `mrStore.load`, which is bound to the sidebar's `stateFilter` and would
  /// only surface open MRs in the palette autocomplete. The underlying
  /// `cacheStore.loadMrList(tabId, 'all')` is cached per tab, so a second
  /// visit short-circuits. The MR target section renders a spinner while
  /// `mrStore.allLoading` is true.
  async function ensureMrList() {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    if (mrStore.mrFeature?.enabled === false) return;
    try { await mrStore.loadAll(tabId); } catch { /* surfaced via mrStore.error */ }
  }

  /// Search Linear/Jira tickets directly against the provider IPC. Bypasses
  /// `issuesStore.loadIssues` on purpose: that path applies the per-repo
  /// default project filter and user-selected sidebar filters, both of which
  /// would scope a palette-level query to whatever the current tab happens
  /// to be configured for. The palette intentionally has no project filter
  /// (per UX), so we pass through a fresh empty-filter object with just the
  /// raw query — the backend interprets the `#`/`~` prefixes natively.
  async function runIssueSearch(kind: 'linear-issue' | 'jira-issue', q: string) {
    const filters = { assigneeMe: false, statusIds: [], labelIds: [], issueTypeIds: [], query: q.trim() || undefined };
    if (kind === 'linear-issue') {
      linearLoading = true;
      try   { linearIssues = await linearSearchIssues(filters); }
      catch { linearIssues = []; }
      finally { linearLoading = false; }
    } else {
      jiraLoading = true;
      try   { jiraIssues = await jiraSearchIssues(filters); }
      catch { jiraIssues = []; }
      finally { jiraLoading = false; }
    }
  }

  /// One-shot fetch of the worktree list for the active tab. Mirrors
  /// `ensureProjectFiles` — cached for the lifetime of the palette open.
  async function ensureWorktrees() {
    if (worktreesLoaded) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    try {
      worktrees = await listWorktrees(tabId);
    } catch {
      worktrees = [];
    }
    worktreesLoaded = true;
  }

  /// Seed `commits` from the already-loaded graph so the commit target picker
  /// shows results the instant a commit-target verb is entered — without it
  /// the dropdown stays empty until the user has typed two characters (the
  /// minimum length `search_commits` accepts). Only fills when `commits` is
  /// empty, so a successful search isn't clobbered between debounces.
  function seedCommitsFromGraph() {
    const nodes = graphStore.graphData?.nodes;
    if (!nodes || nodes.length === 0) return;
    commits = nodes.slice(0, 30).map(n => ({
      oid:        n.oid,
      short_oid:  n.short_oid,
      summary:    n.summary,
      author:     n.author,
      timestamp:  n.timestamp,
    }));
  }

  async function loadCommits(q: string) {
    const tabId = tabsStore.activeTabId;
    // Below the 2-char minimum search_commits accepts: keep the graph-seeded
    // list visible (re-seeded here in case `commits` was cleared by an
    // earlier query) rather than blanking the dropdown.
    if (!tabId || q.length < 2) { seedCommitsFromGraph(); return; }
    try {
      commits = await invoke<SearchResult[]>('search_commits', {
        tabId,
        query: { text: q, include_author: true, limit: 12 },
      });
    } catch { commits = []; }
  }

  // ── Verb system (verb-first model) ─────────────────────────────────────────
  //
  // The palette is strictly verb-first. In Phase 1 the list shows verbs + leaf
  // actions. Selecting a verb transitions to Phase 2 where the input gets a
  // verb chip prefix and the list shows the targets for that verb (branches,
  // tags, commits, files). Backspace on an empty query clears the chip.
  //
  // Shortcut: typing `<verb> ` (word + space) or `<verb>:` auto-promotes — the
  // chip is inserted and the rest of the query is kept as the target filter.

  type TargetKind =
    | 'branch' | 'tag' | 'commit' | 'project-file'
    | 'stash'  | 'remote' | 'tab' | 'recent' | 'mr' | 'theme'
    | 'workspace' | 'ws-repo' | 'ws-repo-any' | 'worktree'
    | 'linear-issue' | 'jira-issue';

  interface VerbDef {
    id:         string;      // unique id, also item id for Phase 1
    title:      string;      // display name (canonical)
    subtitle?:  string;      // one-liner description
    icon:       string;      // lucide icon key
    aliases:    string[];    // extra names accepted by search + auto-promote
    targetKind: TargetKind;
    group:      string;      // Phase 1 section label
    /** Inline executor — each verb owns its logic instead of routing
     *  through a central switch.  Mirrors the `closeAndDispatch` pattern
     *  used for leaf actions.  Receives the chosen target; closures
     *  capture `onClose`, `tabsStore`, `uiStore`, etc. from the component
     *  scope. */
    run:        (target: unknown) => void | Promise<void>;
  }

  // ── Verb runner helpers — shared guards & utilities used by the inline
  //    `run` closures in VERBS below.  Pulled up so we don't repeat them
  //    inside each verb definition. ──────────────────────────────────────
  function requireTab(): string | null {
    const tabId = tabsStore.activeTabId;
    if (!tabId) { onClose(); return null; }
    return tabId;
  }

  async function refreshAfterReset(tabId: string) {
    const [gd, s] = await Promise.all([
      getGraph(tabId, 0, 500),
      getStatus(tabId).catch(() => null),
    ]);
    graphStore.setGraph(gd);
    graphStore.refresh();
    if (s) repoStore.setStatus(s);
  }

  async function applyReset(c: SearchResult, mode: 'soft' | 'mixed' | 'hard') {
    const tabId = requireTab();
    if (!tabId) return;
    const doReset = async () => {
      try {
        await resetToCommit(tabId, c.oid, mode);
        uiStore.showToast(`Reset (${mode}) to ${c.short_oid}`, 'success');
        await refreshAfterReset(tabId);
      } catch (e) {
        uiStore.showToast(`Reset failed: ${e}`, 'error');
      }
      onClose();
    };
    if (mode === 'hard') {
      askConfirm({
        title: 'Hard-reset HEAD',
        message: `Hard-reset HEAD to ${c.short_oid}?`,
        detail: 'This will DISCARD all uncommitted changes.',
        variant: 'danger',
        confirmLabel: 'Reset hard',
        onConfirm: doReset,
      });
      return;
    }
    await doReset();
  }

  async function applyStash(s: StashEntry, op: 'apply' | 'pop' | 'drop') {
    const tabId = requireTab();
    if (!tabId) return;
    try {
      if (op === 'apply') {
        const r = await stashApply(tabId, s.index);
        if (r.blocking_untracked.length > 0) uiStore.showToast('Apply blocked — untracked files conflict', 'warning');
        else if (r.has_conflicts)           uiStore.showToast('Stash applied with conflicts', 'warning');
        else if (r.no_changes)              uiStore.showToast('No changes — working tree already matches the stash', 'info');
        else                                 uiStore.showToast('Stash applied', 'success');
      } else if (op === 'pop') {
        const r = await stashPop(tabId, s.index);
        if (r.blocking_untracked.length > 0) uiStore.showToast('Pop blocked — untracked files conflict', 'warning');
        else if (r.has_conflicts)           uiStore.showToast('Stash popped with conflicts', 'warning');
        else if (r.no_changes)              uiStore.showToast('No changes — working tree already matches the stash. Stash dropped.', 'info');
        else                                 uiStore.showToast('Stash popped', 'success');
      } else {
        askConfirm({
          title: 'Drop stash',
          message: `Drop stash "${s.message}"?`,
          detail: 'This cannot be undone.',
          variant: 'danger',
          confirmLabel: 'Drop',
          onConfirm: async () => {
            try {
              await stashDrop(tabId, s.index);
              uiStore.showToast('Stash dropped', 'success');
              await applyPostStashChange(tabId);
            } catch (e) {
              uiStore.showToast(`Stash drop: ${e}`, 'error');
            }
            onClose();
          },
        });
        return;
      }
      // Repaint stash markers + sidebar list so the change is reflected
      // immediately — palette closes on the next line so the user otherwise
      // wouldn't see the update until they triggered something else.
      await applyPostStashChange(tabId);
    } catch (e) {
      uiStore.showToast(`Stash ${op}: ${e}`, 'error');
    }
    onClose();
  }

  /** Create a branch and refresh — used by the `create-branch` verb in both
   *  steps. Mirrors the modal flow in CommitGraph.doCreateBranch: when the
   *  parent is HEAD, also checkout the new branch. */
  async function runCreateBranch(name: string, fromOid: string, fromHead: boolean) {
    const tabId = requireTab(); if (!tabId) return;
    const trimmed = name.trim();
    if (!trimmed) {
      uiStore.showToast('Branch name cannot be empty', 'warning');
      return;
    }
    const policyError = assertBranchNameAllowed(trimmed, branchPolicy);
    if (policyError) {
      uiStore.showToast(policyError, 'warning');
      return;
    }
    try {
      await createBranch(tabId, trimmed, fromOid);
      if (fromHead) {
        await checkoutBranch(tabId, trimmed);
        uiStore.showToast(`Branch '${trimmed}' created and checked out`, 'success');
      } else {
        uiStore.showToast(`Branch '${trimmed}' created`, 'success');
      }
      graphStore.refresh();
      onClose();
    } catch (e) {
      uiStore.showToast(`Create branch: ${e}`, 'error');
    }
  }

  /** Opens a workspace repo via the arbor:open-ws-repo event — same
   *  pattern as the leaf `closeAndDispatch` actions. */
  function openWsRepoAction(r: RepoRegistryEntry, sourceWsId: string) {
    const isActive = sourceWsId === workspacesStore.activeId;
    const detail = { repoId: r.id, path: r.path, sourceWsId: isActive ? null : sourceWsId };
    onClose();
    tick().then(() => window.dispatchEvent(new CustomEvent('arbor:open-ws-repo', { detail })));
  }

  const VERBS: VerbDef[] = [
    // ── Branch ──────────────────────────────────────────────────────────────
    { id: 'checkout', title: 'Checkout', subtitle: 'Switch to a branch', icon: 'GitBranch', aliases: ['co', 'switch', 'sw', 'ck'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const b = target as BranchInfo;
        const tabId = requireTab(); if (!tabId) return;
        if (b.is_head) { onClose(); return; }
        try {
          const result = await checkoutBranchSafe(tabId, b.name);
          handleCheckoutResult(result, {
            targetLabel:    b.name,
            successMessage: `Checked out ${b.name}`,
          });
          await applyPostCheckout(tabId);
          onClose();
        }
        catch (e) {
          await applyPostCheckout(tabId).catch(() => { /* best-effort */ });
          uiStore.showToast(`Checkout failed: ${e}`, 'error');
        }
      },
    },
    { id: 'merge', title: 'Merge', subtitle: 'Merge a branch into HEAD (default strategy — fast-forward when possible)', icon: 'GitMerge', aliases: ['merge-default'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const tabId = requireTab(); if (!tabId) return;
        await runMergeStrategy(tabId, target as BranchInfo, 'default', 'Merge');
      },
    },
    { id: 'merge-no-ff', title: 'Merge (no fast-forward)', subtitle: 'Always create a merge commit, even when fast-forward is possible', icon: 'GitMerge', aliases: ['no-ff', 'noff'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const tabId = requireTab(); if (!tabId) return;
        await runMergeStrategy(tabId, target as BranchInfo, 'no_ff', 'Merge (no-ff)');
      },
    },
    { id: 'merge-squash', title: 'Squash Merge', subtitle: 'Collapse the branch into a single staged change — review and commit from Stage', icon: 'Combine', aliases: ['squash'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const tabId = requireTab(); if (!tabId) return;
        await runMergeStrategy(tabId, target as BranchInfo, 'squash', 'Squash');
      },
    },
    { id: 'merge-ff-only', title: 'Fast-forward Only', subtitle: 'Move HEAD forward only — fail if a real merge would be needed', icon: 'FastForward', aliases: ['ff', 'ff-only'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const tabId = requireTab(); if (!tabId) return;
        await runMergeStrategy(tabId, target as BranchInfo, 'ff_only', 'Fast-forward');
      },
    },
    { id: 'delete-branch', title: 'Delete Branch', subtitle: 'Remove a local branch', icon: 'Trash2', aliases: ['del', 'rm', 'delb'], targetKind: 'branch', group: 'Branch',
      run: (target) => {
        const b = target as BranchInfo;
        const tabId = requireTab(); if (!tabId) return;
        if (b.is_head) { uiStore.showToast("Can't delete the current branch", 'warning'); onClose(); return; }
        askConfirm({
          title: 'Delete branch',
          message: `Delete branch "${b.name}"?`,
          variant: 'danger',
          confirmLabel: 'Delete',
          onConfirm: async () => {
            try { await deleteBranch(tabId, b.name); uiStore.showToast(`Deleted branch "${b.name}"`, 'success'); }
            catch (e) { uiStore.showToast(`Delete failed: ${e}`, 'error'); }
            onClose();
          },
        });
      },
    },
    { id: 'rename-branch', title: 'Rename Branch', subtitle: 'Rename a local branch', icon: 'Pencil', aliases: ['ren', 'mv'], targetKind: 'branch', group: 'Branch',
      run: (target) => {
        const b = target as BranchInfo;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:rename-branch', { detail: { branch: b } })));
      },
    },
    { id: 'push-branch', title: 'Push Branch', subtitle: 'Push a branch to origin', icon: 'ArrowUpToLine', aliases: ['pushb'], targetKind: 'branch', group: 'Branch',
      run: async (target) => {
        const b = target as BranchInfo;
        const tabId = requireTab(); if (!tabId) return;
        try { await pushBranch(tabId, 'origin', `refs/heads/${b.name}`); uiStore.showToast(`Pushed "${b.name}" to origin`, 'success'); }
        catch (e) { uiStore.showToast(`Push failed: ${e}`, 'error'); }
        onClose();
      },
    },
    { id: 'focus-branch', title: 'Focus Branch in Graph', subtitle: 'Center the graph on a branch HEAD', icon: 'Crosshair', aliases: ['focus', 'goto', 'go', 'show'], targetKind: 'branch', group: 'Navigate',
      run: (target) => {
        const b = target as BranchInfo;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:show-commit', { detail: { oid: b.head_oid } })));
      },
    },

    // ── Navigate ────────────────────────────────────────────────────────────
    { id: 'goto-tag', title: 'Go to Tag', subtitle: 'Center the graph on a tag', icon: 'Tag', aliases: ['tag', 'tags'], targetKind: 'tag', group: 'Navigate',
      run: (target) => {
        const t = target as TagInfo;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:show-commit', { detail: { oid: t.target_oid } })));
      },
    },
    { id: 'goto-commit', title: 'Go to Commit', subtitle: 'Search commits by message, author or hash', icon: 'GitCommit', aliases: ['commit', 'commits'], targetKind: 'commit', group: 'Navigate',
      run: (target) => {
        const c = target as SearchResult;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:show-commit', { detail: { oid: c.oid } })));
      },
    },
    // ── Project-file verbs ────────────────────────────────────────────────
    // These two operate on ANY file tracked in the repo (not just modified)
    // and deliberately do NOT force the File Tree sidebar open — the user can
    // pick a file and either inspect its blame or filter the graph by its
    // history without disturbing the current sidebar selection.
    { id: 'blame-file', title: 'Blame File', subtitle: 'Open Git blame for any file in the project', icon: 'User', aliases: ['blame', 'annotate'], targetKind: 'project-file', group: 'Navigate',
      run: (target) => {
        const f = target as { path: string };
        const tabId = requireTab(); if (!tabId) return;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:show-blame', { detail: { tabId, path: f.path } })));
      },
    },
    { id: 'show-commits-for-file', title: 'Show Commits Touching File', subtitle: 'Filter the graph by a file from anywhere in the project', icon: 'GitCommit', aliases: ['file-history', 'log-file', 'history'], targetKind: 'project-file', group: 'Navigate',
      run: (target) => {
        const f = target as { path: string };
        graphStore.filterByFile(f.path);
        if (uiStore.activeBottomSection === null) uiStore.setActiveBottomSection('detail');
        onClose();
      },
    },

    // ── Commit-target verbs ────────────────────────────────────────────────
    { id: 'cherry-pick', title: 'Cherry-pick', subtitle: 'Apply a commit onto HEAD', icon: 'GitCommit', aliases: ['cp', 'pick'], targetKind: 'commit', group: 'Commit',
      run: async (target) => {
        const c = target as SearchResult;
        const tabId = requireTab(); if (!tabId) return;
        try {
          const r = await cherryPick(tabId, c.oid);
          if (r.has_conflicts) {
            uiStore.showToast('Cherry-pick produced conflicts — resolve in Stage', 'warning');
          } else if (r.no_changes) {
            uiStore.showToast(
              `Cherry-pick di ${c.short_oid} senza modifiche — già presente su HEAD`,
              'info',
            );
          } else {
            uiStore.showToast(`Cherry-picked ${c.short_oid} — rivedi e committa dalla Stage`, 'success');
          }
          await refreshAfterReset(tabId);
        } catch (e) { uiStore.showToast(`Cherry-pick failed: ${e}`, 'error'); }
        onClose();
      },
    },
    { id: 'revert', title: 'Revert Commit', subtitle: 'Create a commit that undoes another', icon: 'Undo2', aliases: ['rv'], targetKind: 'commit', group: 'Commit',
      run: async (target) => {
        const c = target as SearchResult;
        const tabId = requireTab(); if (!tabId) return;
        try {
          const r = await revertCommit(tabId, c.oid);
          if (r.has_conflicts) uiStore.showToast('Revert produced conflicts — resolve in Stage', 'warning');
          else                 uiStore.showToast(`Reverted ${c.short_oid}`, 'success');
          await refreshAfterReset(tabId);
        } catch (e) { uiStore.showToast(`Revert failed: ${e}`, 'error'); }
        onClose();
      },
    },
    { id: 'reset-soft',  title: 'Reset Soft',  subtitle: 'Move HEAD, keep index + workdir',               icon: 'Rewind', aliases: ['rs'],  targetKind: 'commit', group: 'Commit',
      run: (target) => applyReset(target as SearchResult, 'soft') },
    { id: 'reset-mixed', title: 'Reset Mixed', subtitle: 'Move HEAD, reset index, keep workdir',          icon: 'Rewind', aliases: ['rm-'], targetKind: 'commit', group: 'Commit',
      run: (target) => applyReset(target as SearchResult, 'mixed') },
    { id: 'reset-hard',  title: 'Reset Hard',  subtitle: 'Move HEAD + index + workdir (destructive)',     icon: 'Rewind', aliases: ['rh'],  targetKind: 'commit', group: 'Commit',
      run: (target) => applyReset(target as SearchResult, 'hard') },
    // Unified create-branch verb. Step 1: type the name (Enter creates from
    // HEAD). Step 2 (Tab): pick a different parent commit. The verb's `run`
    // is the step-2 path; step-1 items wire their own action directly.
    { id: 'create-branch', title: 'Create Branch', subtitle: 'Type a name; Enter creates from HEAD, Tab picks parent', icon: 'GitBranch', aliases: ['nb', 'newb', 'cb', 'branch', 'bf'], targetKind: 'commit', group: 'Branch',
      run: (target) => {
        const c = target as SearchResult;
        runCreateBranch(pendingBranchName, c.oid, false);
      },
    },
    { id: 'tag-here', title: 'Create Tag', subtitle: 'Type "here" / Enter for HEAD, or pick a commit', icon: 'Tag', aliases: ['th', 'tag-here', 'create-tag'], targetKind: 'commit', group: 'Commit',
      run: (target) => {
        const c = target as SearchResult;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:create-tag-at', { detail: { oid: c.oid } })));
      },
    },
    { id: 'copy-sha', title: 'Copy Commit SHA', subtitle: 'Copy the full OID to clipboard', icon: 'Hash', aliases: ['sha'], targetKind: 'commit', group: 'Commit',
      run: (target) => { const c = target as SearchResult; copyText(c.oid, 'Commit SHA'); },
    },

    // ── Stash ──────────────────────────────────────────────────────────────
    { id: 'stash-apply', title: 'Apply Stash', subtitle: 'Apply a stash without dropping it', icon: 'Archive', aliases: ['apply'], targetKind: 'stash', group: 'Stash',
      run: (target) => applyStash(target as StashEntry, 'apply') },
    { id: 'stash-pop',   title: 'Pop Stash',   subtitle: 'Apply and drop a stash',            icon: 'Archive', aliases: ['pop'],   targetKind: 'stash', group: 'Stash',
      run: (target) => applyStash(target as StashEntry, 'pop') },
    { id: 'stash-drop',  title: 'Drop Stash',  subtitle: 'Delete a stash (destructive)',      icon: 'Trash2',  aliases: ['drop'],  targetKind: 'stash', group: 'Stash',
      run: (target) => applyStash(target as StashEntry, 'drop') },

    // ── Tag ────────────────────────────────────────────────────────────────
    { id: 'delete-tag-local', title: 'Delete Tag (local)', subtitle: 'Remove the tag from this repo only', icon: 'Trash2', aliases: ['delt', 'rmt'], targetKind: 'tag', group: 'Tag',
      run: (target) => {
        const t = target as TagInfo;
        const tabId = requireTab(); if (!tabId) return;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:delete-tag', { detail: { tabId, name: t.name, scope: 'local' } })));
      },
    },
    { id: 'delete-tag-remote', title: 'Delete Tag (local + origin)', subtitle: 'Also push a delete refspec to origin', icon: 'Trash2', aliases: ['delto', 'rmto'], targetKind: 'tag', group: 'Tag',
      run: (target) => {
        const t = target as TagInfo;
        const tabId = requireTab(); if (!tabId) return;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:delete-tag', { detail: { tabId, name: t.name, scope: 'remote' } })));
      },
    },
    { id: 'push-tag', title: 'Push Tag', subtitle: 'Push a tag to origin', icon: 'ArrowUpToLine', aliases: ['pusht'], targetKind: 'tag', group: 'Tag',
      run: async (target) => {
        const t = target as TagInfo;
        const tabId = requireTab(); if (!tabId) return;
        try { await pushBranch(tabId, 'origin', `refs/tags/${t.name}`); uiStore.showToast(`Pushed tag "${t.name}" to origin`, 'success'); }
        catch (e) { uiStore.showToast(`Push tag failed: ${e}`, 'error'); }
        onClose();
      },
    },

    // ── Remote ─────────────────────────────────────────────────────────────
    { id: 'fetch-remote', title: 'Fetch from Remote', subtitle: 'Fetch refs from a specific remote', icon: 'Download', aliases: ['fr'], targetKind: 'remote', group: 'Remote',
      run: async (target) => {
        const r = target as RemoteInfo;
        const tabId = requireTab(); if (!tabId) return;
        try {
          await fetchRemote(tabId, r.name);
          uiStore.showToast(`Fetched from "${r.name}"`, 'success');
          // Refresh graph/sidebar if the fetch actually changed any ref.
          await cacheStore.refreshIfChanged(tabId);
        }
        catch (e) { uiStore.showToast(`Fetch failed: ${e}`, 'error'); }
        onClose();
      },
    },
    { id: 'pull-remote', title: 'Pull from Remote', subtitle: 'Pull current branch from remote', icon: 'ArrowDownToLine', aliases: ['pr'], targetKind: 'remote', group: 'Remote',
      run: async (target) => {
        const r = target as RemoteInfo;
        const tabId = requireTab(); if (!tabId) return;
        // Detached HEAD has no upstream — pull would error out at git level.
        // Mirror the sidebar pull guard.  `current_branch` is populated even
        // in detached HEAD (it's the abbreviated SHA), so check `is_detached`.
        const branchLabel = repoStore.status?.current_branch ?? null;
        if (!branchLabel || repoStore.status?.is_detached) {
          uiStore.showToast(
            'Pull non disponibile in detached HEAD — fai il checkout di un branch',
            'warning',
          );
          onClose();
          return;
        }
        const opId = `pull-${tabId}-${Date.now()}`;
        const tabName = tabsStore.tabs.find(t => t.id === tabId)?.name ?? 'Repository';
        startPullOperation(opId, tabName, branchLabel);
        try {
          const result = await pullBranch(tabId, r.name, opId);
          // The backend returns Ok(PullResult) even on non-clean outcomes
          // (stash conflicts, apply errors) so a bare success toast would
          // lie to the user. Route through the shared handler — it opens
          // the conflict modal / pushes persistent notifications and
          // returns true only on a fully clean pull.
          const st = await getStatus(tabId).catch(() => undefined);
          if (handlePullResult(result, { remoteLabel: r.name, status: st ?? undefined })) {
            uiStore.showToast(`Pulled from "${r.name}"`, 'success');
          }
        } catch (e) {
          const st = await getStatus(tabId).catch(() => null);
          if (!handlePullThrown(e, st, { remoteLabel: r.name })) {
            uiStore.showToast(`Pull failed: ${e}`, 'error');
          }
        }
        onClose();
      },
    },
    { id: 'push-to-remote', title: 'Push Branch to Remote', subtitle: 'Push current branch to a remote', icon: 'ArrowUpToLine', aliases: ['ptr'], targetKind: 'remote', group: 'Remote',
      run: async (target) => {
        const r = target as RemoteInfo;
        const tabId = requireTab(); if (!tabId) return;
        const head = branches.find(x => x.is_head);
        if (!head) { uiStore.showToast('No current branch to push', 'warning'); onClose(); return; }
        try { await pushBranch(tabId, r.name, `refs/heads/${head.name}`); uiStore.showToast(`Pushed "${head.name}" to "${r.name}"`, 'success'); }
        catch (e) { uiStore.showToast(`Push failed: ${e}`, 'error'); }
        onClose();
      },
    },

    // ── Tab ────────────────────────────────────────────────────────────────
    { id: 'switch-tab', title: 'Switch Tab', subtitle: 'Activate a specific repository tab', icon: 'ArrowLeftRight', aliases: ['tab'], targetKind: 'tab', group: 'Tabs',
      run: (target) => { tabsStore.setActive((target as RepoTab).id); onClose(); },
    },
    { id: 'close-tab', title: 'Close Tab', subtitle: 'Close a specific repository tab', icon: 'X', aliases: ['closet'], targetKind: 'tab', group: 'Tabs',
      run: (target) => { tabsStore.removeTab((target as RepoTab).id); onClose(); },
    },

    // ── Recent repos ───────────────────────────────────────────────────────
    { id: 'open-recent', title: 'Open Recent Repository', subtitle: 'Open a repo from the recent list', icon: 'Clock', aliases: ['recent', 'open'], targetKind: 'recent', group: 'Repository',
      run: (target) => {
        const p = target as { path: string };
        onClose();
        tick().then(() => document.dispatchEvent(new CustomEvent('open-recent', { detail: p.path })));
      },
    },

    // ── Appearance ─────────────────────────────────────────────────────────
    { id: 'switch-theme', title: 'Switch Theme', subtitle: 'Apply a built-in or custom theme', icon: 'Palette', aliases: ['theme', 'colors'], targetKind: 'theme', group: 'Appearance',
      run: async (target) => {
        const t = target as Theme;
        try { await themeStore.setActive(t.id); uiStore.showToast(`Theme: ${t.name}`, 'success', 1600); }
        catch (e) { uiStore.showToast(`Theme switch failed: ${e}`, 'error'); }
        onClose();
      },
    },

    // ── Workspaces ─────────────────────────────────────────────────────────
    { id: 'open-project', title: 'Open Project', subtitle: 'Open a repo from the active workspace', icon: 'FolderOpen', aliases: ['proj', 'project', 'openp'], targetKind: 'ws-repo', group: 'Workspaces',
      run: (target) => {
        const t = target as { registry: RepoRegistryEntry; sourceWsId: string };
        openWsRepoAction(t.registry, t.sourceWsId);
      },
    },
    { id: 'open-from-workspace', title: 'Open from Workspace', subtitle: 'Open a repo from any other workspace (as a cross-workspace tab)', icon: 'Layers', aliases: ['openw', 'fromws'], targetKind: 'ws-repo-any', group: 'Workspaces',
      run: (target) => {
        const t = target as { registry: RepoRegistryEntry; sourceWsId: string };
        openWsRepoAction(t.registry, t.sourceWsId);
      },
    },
    { id: 'switch-workspace', title: 'Switch Workspace', subtitle: 'Activate a different workspace', icon: 'Layers', aliases: ['ws', 'workspace', 'sw-ws'], targetKind: 'workspace', group: 'Workspaces',
      run: async (target) => {
        const ws = target as WorkspaceDef;
        onClose();
        if (ws.id === workspacesStore.activeId) return;
        try { await workspacesStore.setActive(ws.id); uiStore.showToast(`Workspace: ${ws.name}`, 'success', 1600); }
        catch (e) { uiStore.showToast(`Switch failed: ${e}`, 'error'); }
      },
    },

    // ── Worktrees ──────────────────────────────────────────────────────────
    { id: 'worktree-info', title: 'Worktree Info', subtitle: 'Open the info panel for a worktree', icon: 'Info', aliases: ['wt', 'wtinfo', 'worktree'], targetKind: 'worktree', group: 'Worktrees',
      run: (target) => {
        const wt = target as WorktreeInfo;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:show-worktree-info', { detail: { worktree: wt } })));
      },
    },
    { id: 'worktree-switch', title: 'Switch Worktree', subtitle: 'Activate another worktree of this project', icon: 'ArrowLeftRight', aliases: ['wts', 'switch-wt'], targetKind: 'worktree', group: 'Worktrees',
      run: async (target) => {
        const wt = target as WorktreeInfo;
        onClose();
        await switchToWorktree(wt);
      },
    },

    // ── Deep Links — `arbor://…` URLs the user can paste/share ─────────────
    // The active tab's first remote is embedded as `?url=` so the link
    // resolves on any machine; if there's no remote, `copyDeepLink` toasts
    // a warning rather than producing an unshareable link.
    { id: 'link-commit', title: 'Copy arbor:// Link to Commit', subtitle: 'Build an arbor:// link that jumps to a commit', icon: 'Link2', aliases: ['linkc', 'dl-commit'], targetKind: 'commit', group: 'Deep Links',
      run: async (target) => {
        const c = target as SearchResult;
        const tabId = requireTab(); if (!tabId) return;
        await copyDeepLink({ kind: 'commit_jump', sha: c.oid }, tabId);
        onClose();
      },
    },
    { id: 'link-branch-checkout', title: 'Copy arbor:// Link to Checkout Branch', subtitle: 'Build an arbor:// link that checks out a branch', icon: 'Link2', aliases: ['linkb', 'dl-checkout'], targetKind: 'branch', group: 'Deep Links',
      run: async (target) => {
        const b = target as BranchInfo;
        const tabId = requireTab(); if (!tabId) return;
        await copyDeepLink({ kind: 'branch_checkout', branch: b.name }, tabId);
        onClose();
      },
    },
    { id: 'link-branch-worktree', title: 'Copy arbor:// Link to Branch Worktree', subtitle: 'Build an arbor:// link that opens a branch as a worktree', icon: 'Link2', aliases: ['linkw', 'dl-worktree'], targetKind: 'branch', group: 'Deep Links',
      run: async (target) => {
        const b = target as BranchInfo;
        const tabId = requireTab(); if (!tabId) return;
        await copyDeepLink({ kind: 'branch_worktree', branch: b.name }, tabId);
        onClose();
      },
    },
    // ── Merge Requests ─────────────────────────────────────────────────────
    { id: 'mr-detail', title: 'View MR / PR Detail', subtitle: 'Open the pull / merge request detail modal', icon: 'GitPullRequest', aliases: ['mr', 'mrd', 'prd', 'mr-detail', 'pr-detail', 'view-mr', 'view-pr', 'open-mr', 'open-pr'], targetKind: 'mr', group: 'Merge Requests',
      run: (target) => {
        const mr = target as MergeRequest;
        onClose();
        tick().then(() => window.dispatchEvent(new CustomEvent('arbor:open-mr-detail', { detail: { mr } })));
      },
    },
    { id: 'link-mr', title: 'Copy arbor:// Link to MR', subtitle: 'Build an arbor:// link to a pull / merge request', icon: 'Link2', aliases: ['linkmr', 'dl-mr'], targetKind: 'mr', group: 'Deep Links',
      run: async (target) => {
        const mr = target as MergeRequest;
        const tabId = requireTab(); if (!tabId) return;
        await copyDeepLink({ kind: 'mr_open', number: mr.number }, tabId);
        onClose();
      },
    },

    // ── Issues (cross-tab, gated on per-provider auth) ─────────────────────
    // These query Linear/Jira directly via IPC — independent of the current
    // tab's `issue_tracker` config — and open the detail modal pinned to
    // the picked provider. Hidden in Phase 1 (and in `findVerbByWord`)
    // when the user isn't authenticated to the provider.
    { id: 'linear-issue', title: 'Linear Issue', subtitle: 'Search Linear tickets and open detail (# for code, ~ for text)', icon: 'Sparkles', aliases: ['linear', 'lin'], targetKind: 'linear-issue', group: 'Issues',
      run: (target) => {
        const issue = target as Issue;
        onClose();
        tick().then(() => issuesStore.selectAndLoadIssue(issue, 'linear'));
      },
    },
    { id: 'jira-issue', title: 'Jira Issue', subtitle: 'Search Jira tickets and open detail (# for code, ~ for text)', icon: 'Sparkles', aliases: ['jira', 'jir'], targetKind: 'jira-issue', group: 'Issues',
      run: (target) => {
        const issue = target as Issue;
        onClose();
        tick().then(() => issuesStore.selectAndLoadIssue(issue, 'jira'));
      },
    },
  ];

  /** A verb is hidden when its target source isn't reachable — MR/PRs
   *  disabled on the active repo, or the user isn't signed in to a tracker.
   *  Single predicate so Phase 1 listing and `findVerbByWord` auto-promote
   *  stay in lockstep. */
  function isVerbAvailable(v: VerbDef): boolean {
    if (v.targetKind === 'mr' && mrStore.mrFeature?.enabled === false) return false;
    if (v.targetKind === 'linear-issue' && !linearAuthed) return false;
    if (v.targetKind === 'jira-issue'   && !jiraAuthed)   return false;
    return true;
  }

  /** Match a verb by its id, canonical title or any alias (case-insensitive).
   *  Hidden verbs (MR disabled, tracker not authed) never auto-promote — so
   *  typing "linear " when signed out doesn't enter an unreachable chip. */
  function findVerbByWord(word: string): VerbDef | null {
    const w = word.toLowerCase();
    return VERBS.find(v =>
      isVerbAvailable(v) &&
      (v.id === w
        || v.title.toLowerCase() === w
        || v.aliases.includes(w)),
    ) ?? null;
  }

  /** Run a verb against its target.  Each VerbDef owns its `run` closure
   *  (see VERBS above) — this wrapper is the single call-point used by
   *  palette items so the per-verb logic stays inline in the definitions
   *  instead of re-dispatched through a big switch here. */
  async function executeVerb(verb: VerbDef, target: unknown) {
    await verb.run(target);
  }
  /** Score a verb against its canonical name plus every alias — best match wins. */
  function scoreVerb(v: VerbDef, q: string): number {
    if (!q) return 50;
    let best = score(v.title, q);
    for (const a of v.aliases) {
      const s = score(a, q);
      if (s > best) best = s;
    }
    return best;
  }

  const sections = $derived<PaletteSection[]>(buildSections(query));

  /// True while the targets for the currently-selected verb are being fetched
  /// (MR list today; future async target sources can plug in the same way).
  /// Drives the centred spinner in the results area so users don't see an
  /// empty "No merge requests available" message before the cache warms up.
  const targetsLoading = $derived.by(() => {
    if (!selectedVerb) return false;
    if (selectedVerb.targetKind === 'mr')           return mrStore.allLoading;
    if (selectedVerb.targetKind === 'linear-issue') return linearLoading;
    if (selectedVerb.targetKind === 'jira-issue')   return jiraLoading;
    return false;
  });

  function buildSections(raw: string): PaletteSection[] {
    // Phase 2 — a verb chip is set: show only matching targets.
    if (selectedVerb) return buildTargetSections(selectedVerb, raw);

    // Phase 1 — verb-first: the list is verbs + leaf actions.
    return buildPhaseOneSections(raw);
  }

  // ── Phase 1 — verbs + leaf actions ─────────────────────────────────────────
  function buildPhaseOneSections(q: string): PaletteSection[] {
    const result: PaletteSection[] = [];

    // Group verbs by their declared group so the palette reads as a taxonomy.
    const verbGroups = new Map<string, PaletteItem[]>();
    for (const v of VERBS) {
      // Skip verbs whose target source isn't reachable (MR/PRs disabled,
      // Linear/Jira not authed). Centralised in `isVerbAvailable` so the
      // gate stays in lockstep with `findVerbByWord`.
      if (!isVerbAvailable(v)) continue;
      const s = scoreVerb(v, q);
      if (s <= 0) continue;
      const item: PaletteItem = {
        id:       `verb:${v.id}`,
        kind:     'verb',
        icon:     v.icon,
        title:    v.title,
        subtitle: v.subtitle,
        score:    s,
        action:   () => enterVerb(v),
      };
      const list = verbGroups.get(v.group) ?? [];
      list.push(item);
      verbGroups.set(v.group, list);
    }
    // Emit in insertion order (Map preserves it).
    for (const [label, items] of verbGroups) {
      items.sort((a, b) => b.score - a.score);
      result.push({ id: `verb-group:${label}`, label, items });
    }

    // Leaf actions — grouped by their declared `group` so the user can
    // scan the palette as a taxonomy rather than a flat dump of 40+ rows.
    const actionGroups = new Map<string, PaletteItem[]>();
    for (const a of builtinActions) {
      const s = score(a.title + ' ' + (a.subtitle ?? ''), q);
      if (s <= 0) continue;
      const item: PaletteItem = { ...a, score: s };
      const gname = a.group ?? 'Actions';
      const list = actionGroups.get(gname) ?? [];
      list.push(item);
      actionGroups.set(gname, list);
    }
    // With a query, collapse all groups into a single ranked list so users
    // searching for "commit" or "push" aren't distracted by section headers.
    if (q) {
      const all = [...actionGroups.values()].flat().sort((a, b) => b.score - a.score).slice(0, 12);
      if (all.length) result.push({ id: 'actions', label: 'Actions', items: all });
    } else {
      for (const [label, items] of actionGroups) {
        items.sort((a, b) => b.score - a.score);
        result.push({ id: `action-group:${label}`, label, items });
      }
    }

    // Open With — one launcher per detected / custom IDE.
    if (tabsStore.activeTab) {
      const openWithItems: PaletteItem[] = openWithIdes
        .map(ide => {
          const title = `Open in ${ide.name}`;
          return {
            id:       `open-with:${ide.id}`,
            kind:     'action' as PaletteItemKind,
            icon:     'ExternalLink',
            title,
            subtitle: `Launch ${ide.name} on the current workspace`,
            score:    score(title + ' open with ' + ide.name, q),
            action:   () => openCurrentWorkspace(ide.id),
          };
        })
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, q ? 6 : 8);

      if (openWithItems.length) {
        result.push({ id: 'open-with', label: 'Open With', items: openWithItems });
      }
    }

    // Plugin commands — leaf actions registered by plugins.
    const pluginItems: PaletteItem[] = contributionStore.forPoint('arbor:command-palette')
      .map(c => {
        const p = c.payload as { title?: string; description?: string; icon?: string };
        return {
          id:       `plugin:${c.plugin_name}:${c.item_id}`,
          kind:     'plugin' as PaletteItemKind,
          icon:     p.icon ?? 'Zap',
          title:    p.title ?? '',
          subtitle: p.description ?? c.plugin_name,
          score:    score((p.title ?? '') + ' ' + (p.description ?? '') + ' ' + c.plugin_name, q),
          action: () => {
            firePluginAction(c.plugin_name, `command:${c.item_id}`, '{}').catch(() => {});
            onClose();
          },
        };
      })
      .filter(p => p.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 8);

    if (pluginItems.length) {
      result.push({ id: 'plugins', label: 'Plugin Commands', items: pluginItems });
    }

    return result;
  }

  // ── Phase 2 — target picker for a specific verb ────────────────────────────
  function buildTargetSections(verb: VerbDef, q: string): PaletteSection[] {
    // Two-step "Create Branch" verb has its own UI shape — handled separately
    // so the standard targetKind=='commit' branch below can't fire for it.
    if (verb.id === 'create-branch') {
      return buildCreateBranchSections(q);
    }

    if (verb.targetKind === 'branch') {
      const items: PaletteItem[] = branches
        .map(b => ({
          id:       `target:${verb.id}:${b.name}`,
          kind:     'branch' as PaletteItemKind,
          icon:     'GitBranch',
          title:    b.name,
          subtitle: b.is_head ? 'current branch' : (b.upstream ?? undefined),
          score:    score(b.name, q),
          action:   () => executeVerb(verb, b),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 12);
      return items.length ? [{ id: 'branches', label: 'Branches', items }] : [];
    }

    if (verb.targetKind === 'tag') {
      const items: PaletteItem[] = tags
        .map(t => ({
          id:       `target:${verb.id}:${t.name}`,
          kind:     'commit' as PaletteItemKind,
          icon:     'Tag',
          title:    t.name,
          subtitle: t.message?.split('\n')[0] ?? t.target_oid.slice(0, 8),
          score:    score(t.name + ' ' + (t.message ?? ''), q),
          action:   () => executeVerb(verb, t),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 12);
      return items.length ? [{ id: 'tags', label: 'Tags', items }] : [];
    }

    if (verb.targetKind === 'commit') {
      // Score against summary + short_oid + author so that both backend-
      // searched results (q ≥ 2 chars) and graph-seeded results (q < 2) get
      // narrowed by what the user is typing. Empty `q` yields a neutral
      // score of 50, so the seeded head of the graph stays visible.
      const items: PaletteItem[] = commits.map(c => {
        const s = score(`${c.summary} ${c.short_oid} ${c.author.name}`, q);
        return {
          id:       `target:${verb.id}:${c.oid}`,
          kind:     'commit' as PaletteItemKind,
          icon:     'GitCommit',
          title:    c.summary,
          subtitle: `${c.short_oid} · ${c.author.name}`,
          score:    s,
          action:   () => executeVerb(verb, c),
        };
      })
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 20);

      // `Create Tag` injects a synthetic "here" entry pinned at the top so
      // pressing Enter on an empty query opens the tag modal at HEAD. The
      // entry also surfaces when the user explicitly types "here".
      const sections: PaletteSection[] = [];
      if (verb.id === 'tag-here') {
        const headOid = branches.find(b => b.is_head)?.head_oid ?? null;
        if (headOid) {
          const hereScore = q.trim() ? score('here HEAD current commit', q) : 100;
          if (hereScore > 0) {
            sections.push({
              id: 'tag-here-shortcut',
              label: 'Quick',
              items: [{
                id:       `target:${verb.id}:here`,
                kind:     'commit' as PaletteItemKind,
                icon:     'Tag',
                title:    'here',
                subtitle: `current commit · ${headOid.slice(0, 7)}`,
                score:    hereScore,
                action:   () => {
                  onClose();
                  tick().then(() => window.dispatchEvent(
                    new CustomEvent('arbor:create-tag-at', { detail: { oid: headOid } }),
                  ));
                },
              }],
            });
          }
        }
      }

      if (items.length) sections.push({ id: 'commits', label: 'Commits', items });
      return sections;
    }

    if (verb.targetKind === 'project-file') {
      // Empty query → show a small head of the list as discoverability
      // ("type to filter"). Avoid scoring 0 in that mode by passing a
      // neutral score so all entries make it to the slice.
      const trimmed = q.trim();
      const items: PaletteItem[] = projectFiles.map(p => {
        const s = trimmed ? score(p, q) : 50;
        return {
          id:       `target:${verb.id}:${p}`,
          kind:     'project-file' as PaletteItemKind,
          icon:     'FileText',
          title:    p.split('/').pop() ?? p,
          subtitle: p,
          score:    s,
          action:   () => executeVerb(verb, { path: p }),
        };
      })
      .filter(i => i.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 20);
      return items.length ? [{ id: 'project-files', label: 'Project files', items }] : [];
    }

    if (verb.targetKind === 'stash') {
      const items: PaletteItem[] = stashes
        .map(s => ({
          id:       `target:${verb.id}:${s.index}`,
          kind:     'stash' as PaletteItemKind,
          icon:     'Archive',
          title:    s.message,
          subtitle: `stash@{${s.index}} · ${s.oid.slice(0, 8)}`,
          score:    score(s.message + ' stash@' + s.index, q),
          action:   () => executeVerb(verb, s),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 12);
      return items.length ? [{ id: 'stashes', label: 'Stashes', items }] : [];
    }

    if (verb.targetKind === 'remote') {
      const items: PaletteItem[] = remotes
        .map(r => ({
          id:       `target:${verb.id}:${r.name}`,
          kind:     'remote' as PaletteItemKind,
          icon:     'Globe',
          title:    r.name,
          subtitle: r.url,
          score:    score(r.name + ' ' + r.url, q),
          action:   () => executeVerb(verb, r),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 12);
      return items.length ? [{ id: 'remotes', label: 'Remotes', items }] : [];
    }

    if (verb.targetKind === 'tab') {
      const items: PaletteItem[] = tabsStore.tabs
        .map(t => ({
          id:       `target:${verb.id}:${t.id}`,
          kind:     'tab' as PaletteItemKind,
          icon:     'FolderOpen',
          title:    t.name,
          subtitle: t.id === tabsStore.activeTabId ? `active · ${t.path}` : t.path,
          score:    score(t.name + ' ' + t.path, q),
          action:   () => executeVerb(verb, t),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 15);
      return items.length ? [{ id: 'tabs', label: 'Open Tabs', items }] : [];
    }

    if (verb.targetKind === 'recent') {
      const openPaths = new Set(tabsStore.tabs.map(t => t.path.replace(/\\/g, '/')));
      const items: PaletteItem[] = uiStore.recentRepos
        .filter(p => !openPaths.has(p.replace(/\\/g, '/')))
        .map(p => {
          const name = p.split(/[\\/]/).pop() ?? p;
          return {
            id:       `target:${verb.id}:${p}`,
            kind:     'recent' as PaletteItemKind,
            icon:     'FolderOpen',
            title:    name,
            subtitle: p,
            score:    score(name + ' ' + p, q),
            action:   () => executeVerb(verb, { path: p }),
          };
        })
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 15);
      return items.length ? [{ id: 'recent', label: 'Recent Repositories', items }] : [];
    }

    if (verb.targetKind === 'mr') {
      const items: PaletteItem[] = mrStore.allMrs
        .map(mr => ({
          id:       `target:${verb.id}:${mr.number}`,
          kind:     'mr' as PaletteItemKind,
          icon:     'GitPullRequest',
          title:    `#${mr.number} ${mr.title}`,
          subtitle: `${mr.state} · ${mr.sourceBranch} → ${mr.targetBranch}`,
          score:    score(`#${mr.number} ${mr.title} ${mr.sourceBranch} ${mr.state}`, q),
          action:   () => executeVerb(verb, mr),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 15);
      return items.length ? [{ id: 'mrs', label: 'Merge Requests', items }] : [];
    }

    if (verb.targetKind === 'worktree') {
      const items: PaletteItem[] = worktrees
        .map(wt => {
          const name = wt.branch ?? (wt.path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? wt.path);
          const flags = [
            wt.is_current ? 'current' : null,
            wt.is_main ? 'main' : null,
            wt.is_locked ? 'locked' : null,
            !wt.branch ? 'detached' : null,
          ].filter(Boolean).join(' · ');
          return {
            id:       `target:${verb.id}:${wt.path}`,
            kind:     'action' as PaletteItemKind,
            icon:     'Layers',
            title:    name,
            subtitle: flags ? `${flags} · ${wt.path}` : wt.path,
            score:    score(name + ' ' + wt.path, q),
            action:   () => executeVerb(verb, wt),
          };
        })
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 20);
      return items.length ? [{ id: 'worktrees', label: 'Worktrees', items }] : [];
    }

    if (verb.targetKind === 'theme') {
      const activeId = themeStore.activeId;
      const items: PaletteItem[] = themeStore.allThemes
        .map(t => ({
          id:       `target:${verb.id}:${t.id}`,
          kind:     'theme' as PaletteItemKind,
          icon:     'Palette',
          title:    t.name,
          subtitle: t.id === activeId ? 'active' : t.id,
          score:    score(t.name + ' ' + t.id, q),
          action:   () => executeVerb(verb, t),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 20);
      return items.length ? [{ id: 'themes', label: 'Themes', items }] : [];
    }

    // ── Workspace targets ──────────────────────────────────────────────────
    // Each target carries its own inline `action` closure — no central
    // executeVerb switch — same pattern as the leaf actions above
    // (`arbor:pull` / `arbor:fetch` / …).  The actions either setActive
    // directly or dispatch a window event that AppShell handles.
    if (verb.targetKind === 'workspace') {
      const activeId = workspacesStore.activeId;
      const items: PaletteItem[] = workspacesStore.workspaces
        .map(w => ({
          id:       `target:${verb.id}:${w.id}`,
          kind:     'action' as PaletteItemKind,
          icon:     'Layers',
          title:    w.name,
          subtitle: w.id === activeId ? `active · ${w.repo_ids.length} repos` : `${w.repo_ids.length} repos`,
          score:    score(w.name, q),
          action:   async () => {
            onClose();
            if (w.id === workspacesStore.activeId) return;
            try {
              await workspacesStore.setActive(w.id);
              uiStore.showToast(`Workspace: ${w.name}`, 'success', 1600);
            } catch (e) {
              uiStore.showToast(`Switch failed: ${e}`, 'error');
            }
          },
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score);
      return items.length ? [{ id: 'workspaces', label: 'Workspaces', items }] : [];
    }

    if (verb.targetKind === 'ws-repo') {
      const activeWs = workspacesStore.active;
      if (!activeWs) return [];
      const items: PaletteItem[] = activeWs.repo_ids
        .map(id => workspacesStore.registryById.get(id))
        .filter((r): r is RepoRegistryEntry => !!r)
        .map(r => ({
          id:       `target:${verb.id}:${r.id}`,
          kind:     'action' as PaletteItemKind,
          icon:     'FolderOpen',
          title:    r.display_name,
          subtitle: r.path,
          score:    score(r.display_name + ' ' + r.path, q),
          action:   () => openWsRepoAction(r, activeWs.id),
        }))
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 30);
      return items.length ? [{ id: 'ws-repos', label: `Repositories in ${activeWs.name}`, items }] : [];
    }

    if (verb.targetKind === 'linear-issue' || verb.targetKind === 'jira-issue') {
      const list = verb.targetKind === 'linear-issue' ? linearIssues : jiraIssues;
      // The backend already filtered by `query` (including the `#`/`~`
      // prefix semantics). The local score below is only used to *order*
      // results, so we strip the prefix first — otherwise an item like
      // "ENG-42 fix login" gets score 0 against the literal query "#ENG-42"
      // (the `#` doesn't appear anywhere in identifier/title) and the
      // filter step drops the whole list. Stripping makes the local score
      // line up with whatever the backend was matching against.
      const trimmed = q.trim();
      const scoreQuery = trimmed.startsWith('#') || trimmed.startsWith('~')
        ? trimmed.slice(1).trim()
        : trimmed;
      const items: PaletteItem[] = list.map(i => {
        const s = scoreQuery
          ? score(`${i.identifier} ${i.title}`, scoreQuery)
          : 50;
        return {
          id:       `target:${verb.id}:${i.id}`,
          kind:     'action' as PaletteItemKind,
          icon:     'Sparkles',
          title:    `${i.identifier} — ${i.title}`,
          subtitle: `${i.status.name}${i.assignee ? ` · ${i.assignee.displayName}` : ''}`,
          score:    s,
          action:   () => executeVerb(verb, i),
        };
      })
        .filter(i => i.score > 0)
        .sort((a, b) => b.score - a.score)
        .slice(0, 20);
      const label = verb.targetKind === 'linear-issue' ? 'Linear issues' : 'Jira issues';
      return items.length ? [{ id: verb.targetKind, label, items }] : [];
    }

    if (verb.targetKind === 'ws-repo-any') {
      const activeWsId = workspacesStore.activeId;
      const sections: PaletteSection[] = [];
      for (const ws of workspacesStore.workspaces) {
        if (ws.id === activeWsId) continue; // scoped to OTHER workspaces
        const items: PaletteItem[] = ws.repo_ids
          .map(id => workspacesStore.registryById.get(id))
          .filter((r): r is RepoRegistryEntry => !!r)
          .map(r => ({
            id:       `target:${verb.id}:${ws.id}:${r.id}`,
            kind:     'action' as PaletteItemKind,
            icon:     'FolderOpen',
            title:    r.display_name,
            subtitle: `${ws.name} · ${r.path}`,
            score:    score(r.display_name + ' ' + r.path + ' ' + ws.name, q),
            action:   () => openWsRepoAction(r, ws.id),
          }))
          .filter(i => i.score > 0)
          .sort((a, b) => b.score - a.score)
          .slice(0, 12);
        if (items.length) sections.push({ id: `ws-any-${ws.id}`, label: ws.name, items });
      }
      return sections;
    }

    return [];
  }

  // ── Phase 2 (specialised) — Create Branch ──────────────────────────────────
  //
  // Step 'name'    — `q` IS the branch name. List has a single hint item
  //                  ("Create '<q>' from <HEAD>") whose action runs immediately.
  //                  Tab advances to step 'parent'.
  // Step 'parent'  — `q` filters commits. The HEAD commit is pinned at the top
  //                  as the default parent; below, search results once the
  //                  user types ≥ 2 characters. Backspace on empty rewinds
  //                  to step 'name' with the captured name restored.
  function buildCreateBranchSections(q: string): PaletteSection[] {
    const headBranch = branches.find(b => b.is_head) ?? null;
    const headOid    = headBranch?.head_oid ?? null;
    const fromLabel  = headBranch
      ? `from ${headBranch.name} (${(headOid ?? '').slice(0, 7)})`
      : 'from HEAD';
    const trimmed = q.trim();

    if (createBranchStep === 'name') {
      // Ticket-policy hints. Only enforced when the flag is on AND a tracker
      // is configured — matches the GitFlowPanel rule. A ticket-suggestions
      // section is appended below the action item when the policy is active.
      const ticketHint = branchPolicy.requireTicket && branchPolicy.tracker
        ? ` · ticket key required (${branchPolicy.tracker === 'github' || branchPolicy.tracker === 'gitlab' ? '#123' : 'ABC-123'})`
        : '';
      const ticketSuggestions = buildTicketSuggestionsSection(trimmed);

      const actionItem: PaletteItem = !trimmed
        ? {
            id:       'create-branch:hint',
            kind:     'action' as PaletteItemKind,
            icon:     'GitBranch',
            title:    'Type a branch name…',
            subtitle: `Enter creates ${fromLabel} · Tab picks a different parent${ticketHint}`,
            score:    100,
            action:   () => { /* hint only — no-op */ },
          }
        : {
            id:       'create-branch:from-head',
            kind:     'action' as PaletteItemKind,
            icon:     'GitBranch',
            title:    `Create '${trimmed}' ${fromLabel}`,
            subtitle: `Enter to create · Tab to pick a different parent${ticketHint}`,
            score:    100,
            action:   () => {
              if (!headOid) {
                uiStore.showToast('No HEAD to branch from', 'warning');
                return;
              }
              runCreateBranch(trimmed, headOid, true);
            },
          };

      const result: PaletteSection[] = [{
        id:    'create-branch',
        label: 'New branch',
        items: [actionItem],
      }];
      if (ticketSuggestions) result.push(ticketSuggestions);
      return result;
    }

    // Step 'parent'
    const result: PaletteSection[] = [];
    if (headOid) {
      result.push({
        id:    'create-branch-default',
        label: 'Default parent',
        items: [{
          id:       'create-branch:default-parent',
          kind:     'commit' as PaletteItemKind,
          icon:     'Crosshair',
          title:    headBranch ? `${headBranch.name} (HEAD)` : 'HEAD',
          subtitle: `${headOid.slice(0, 7)} · creates and checks out '${pendingBranchName}'`,
          score:    100,
          action:   () => runCreateBranch(pendingBranchName, headOid, true),
        }],
      });
    }
    if (commits.length) {
      result.push({
        id:    'create-branch-commits',
        label: 'Other parent commits',
        items: commits.map(c => ({
          id:       `create-branch:parent:${c.oid}`,
          kind:     'commit' as PaletteItemKind,
          icon:     'GitCommit',
          title:    c.summary,
          subtitle: `${c.short_oid} · ${c.author.name}`,
          score:    85,
          action:   () => runCreateBranch(pendingBranchName, c.oid, false),
        })),
      });
    }
    return result;
  }

  /** When the require-ticket policy is active, surface matching issues from
   *  the issuesStore as branch-name suggestions. Selecting one fills the
   *  input with the ticket identifier — the user can append a slug afterwards
   *  (e.g. `ABO-123-fix-button`) before pressing Enter. Returns null when
   *  policy is off, no tracker is configured, or no issues are available
   *  (the warm-up in onMount is best-effort). */
  function buildTicketSuggestionsSection(q: string): PaletteSection | null {
    if (!branchPolicy.requireTicket || !branchPolicy.tracker) return null;
    const all = issuesStore.sortedIssues;
    if (all.length === 0) return null;
    const lq = q.toLowerCase();
    const matches = (lq
      ? all.filter(i =>
          i.identifier.toLowerCase().includes(lq) ||
          i.title.toLowerCase().includes(lq))
      : all
    ).slice(0, 8);
    if (matches.length === 0) return null;
    return {
      id:    'create-branch-tickets',
      label: 'Suggested tickets',
      items: matches.map((issue: Issue) => ({
        id:       `create-branch:ticket:${issue.identifier}`,
        kind:     'action' as PaletteItemKind,
        icon:     'Sparkles',
        title:    `${issue.identifier} — ${issue.title}`,
        subtitle: `Use as branch name`,
        score:    90,
        action:   () => {
          query = issue.identifier;
          // Move cursor to end so the user can immediately append a slug.
          tick().then(() => {
            inputEl?.focus();
            if (inputEl) inputEl.selectionStart = inputEl.selectionEnd = query.length;
          });
        },
      })),
    };
  }

  // ── Flat list for keyboard nav ───────────────────────────────────────────────

  const flatItems = $derived(sections.flatMap(s => s.items));

  // ── Ghost text ───────────────────────────────────────────────────────────────

  const ghostSuffix = $derived(computeGhost(query, flatItems));

  function computeGhost(q: string, items: PaletteItem[]): string {
    if (!q || items.length === 0) return '';
    const lq = q.toLowerCase();
    const first = items.find(i => i.title.toLowerCase().startsWith(lq));
    if (!first) return '';
    return first.title.slice(q.length);
  }

  // ── Keep selection in bounds ─────────────────────────────────────────────────

  $effect(() => {
    flatItems; // track
    if (selectedIdx >= flatItems.length) selectedIdx = Math.max(0, flatItems.length - 1);
  });

  // ── Query reactive effects ───────────────────────────────────────────────────

  $effect(() => {
    const q = query;
    const verb = selectedVerb;
    if (autoPromoting) return;

    // Auto-promote: "<verb> " or "<verb>:" at the start becomes a chip.
    if (!verb) {
      const m = q.match(/^(\S+?)(?:\s+|\s*:\s*)(.*)$/);
      if (m) {
        const vp = findVerbByWord(m[1]);
        if (vp) { enterVerb(vp, m[2]); return; }
      }
    }

    // Commit search is only relevant when the selected verb targets commits.
    // NOTE: don't read `commits` here — it mutates when loadCommits resolves
    // and that would loop. Verb transitions clear commits explicitly.
    // For create-branch we must skip search during step='name' (the query
    // there is the branch name, not a commit filter).
    clearTimeout(commitDebounce);
    const isCreateBranchNameStep =
      verb?.id === 'create-branch' && createBranchStep === 'name';
    if (verb?.targetKind === 'commit' && !isCreateBranchNameStep) {
      commitDebounce = setTimeout(() => loadCommits(q), 200);
    }
    // Lazy-fetch the project file list when a project-file verb is active.
    // No-op after the first call thanks to `projectFilesLoaded`.
    if (verb?.targetKind === 'project-file') {
      void ensureProjectFiles();
    }
    if (verb?.targetKind === 'worktree') {
      void ensureWorktrees();
    }
    // Issue verbs — debounced server-side search. The initial empty-query
    // fetch is kicked off by `enterVerb`; this effect handles keystrokes
    // after the chip is already in place.
    clearTimeout(issueDebounce);
    if (verb?.targetKind === 'linear-issue' || verb?.targetKind === 'jira-issue') {
      const kind = verb.targetKind;
      issueDebounce = setTimeout(() => runIssueSearch(kind, q), 250);
    }
    selectedIdx = 0;
  });

  // ── Keyboard handler ─────────────────────────────────────────────────────────

  function onKeydown(e: KeyboardEvent) {
    // Backspace at the very start of the input.
    //   • Default behaviour: removes the verb chip.
    //   • Create-branch step='parent': rewind to step='name' and put the
    //     captured name back into the input so the user can edit it.
    if (e.key === 'Backspace' && selectedVerb && query === '') {
      e.preventDefault();
      if (selectedVerb.id === 'create-branch' && createBranchStep === 'parent') {
        const restore = pendingBranchName;
        pendingBranchName = '';
        createBranchStep = 'name';
        commits = [];
        query = restore;
        selectedIdx = 0;
        tick().then(() => {
          if (inputEl) inputEl.selectionStart = inputEl.selectionEnd = query.length;
        });
      } else {
        clearVerb();
      }
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, flatItems.length - 1);
      scrollIntoView();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      scrollIntoView();
    } else if (
      e.key === 'Tab' &&
      selectedVerb?.id === 'create-branch' &&
      createBranchStep === 'name' &&
      query.trim()
    ) {
      // Capture the typed branch name and switch to parent picker.
      // Must run before the ghost-suffix Tab branch below — when the only
      // list item starts with the query, ghostSuffix would otherwise win.
      e.preventDefault();
      pendingBranchName = query.trim();
      query = '';
      createBranchStep = 'parent';
      selectedIdx = 0;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      flatItems[selectedIdx]?.action();
    } else if (e.key === 'Tab' && ghostSuffix) {
      e.preventDefault();
      // Find the item the ghost is previewing (same lookup as computeGhost).
      // For verb items, run their action so we enter the verb chip cleanly —
      // otherwise filling the multi-word title triggers the auto-promote
      // effect, which splits on the first space and may map the first word
      // to a *different* verb (e.g. "Go to Tag" → focus-branch via 'go' alias).
      const lq = query.toLowerCase();
      const matched = flatItems.find(i => i.title.toLowerCase().startsWith(lq));
      if (matched?.kind === 'verb') {
        matched.action();
      } else {
        query = query + ghostSuffix;
        // Move cursor to end
        tick().then(() => {
          if (inputEl) { inputEl.selectionStart = inputEl.selectionEnd = query.length; }
        });
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  let listEl = $state<HTMLElement | undefined>(undefined);

  function scrollIntoView() {
    tick().then(() => {
      const el = listEl?.querySelector<HTMLElement>(`[data-idx="${selectedIdx}"]`);
      el?.scrollIntoView({ block: 'nearest' });
    });
  }

  // ── Helpers ──────────────────────────────────────────────────────────────────

  /** Close the palette then fire a custom event on the next tick. */
  function closeAndDispatch(event: string) {
    onClose();
    tick().then(() => window.dispatchEvent(new CustomEvent(event)));
  }

  // ── Mount ────────────────────────────────────────────────────────────────────

  onMount(() => {
    inputEl?.focus();
    loadBranchesAndStatus();
    // Probe per-provider auth so the Linear/Jira issue verbs surface only
    // for providers the user is signed in to. Fire-and-forget — failures
    // just leave the corresponding verb hidden, which matches the intent.
    linearGetAuthStatus()
      .then(s => { linearAuthed = !!s.authenticated; })
      .catch(() => { linearAuthed = false; });
    jiraGetAuthStatus()
      .then(s => { jiraAuthed = !!s.authenticated; })
      .catch(() => { jiraAuthed = false; });
    // MR list is loaded lazily on demand — only when the user enters a verb
    // whose `targetKind === 'mr'` (see `ensureMrList`). The list is cached
    // per tab in `cacheStore`, so the second visit is instant.
    const tabId = tabsStore.activeTabId;
    // Resolve branch-creation policy + warm the issues cache when a ticket
    // tracker is configured. Both are best-effort: failures don't block the
    // palette and just leave the suggestions section empty.
    if (tabId) {
      getBranchPolicy(tabId)
        .then(p => {
          branchPolicy = p;
          if (p.requireTicket && p.tracker && issuesStore.issues.length === 0) {
            // Ensure provider + auth are populated before searching.
            // Each step swallows its own error so a failed warm-up just
            // leaves the suggestions section empty.
            issuesStore.loadProviderForTab(tabId)
              .then(() => issuesStore.loadAuthStatus())
              .then(() => issuesStore.loadIssues())
              .catch(() => {});
          }
        })
        .catch(() => {});
    }
    return () => {
      clearTimeout(commitDebounce);
      clearTimeout(issueDebounce);
    };
  });

  // Reactive pending-verb bridge: picks up uiStore.openCommandPaletteWithVerb
  // even when this component was already mounted.  On welcome screen in
  // particular, onMount-only consumption missed the signal because pre-flight
  // focus logic races with Svelte's micro-queue flush — a $effect that
  // watches the store fires deterministically after mount AND on any later
  // shortcut invocation.
  $effect(() => {
    const pending = uiStore.pendingPaletteVerb;
    if (!pending) return;
    const v = VERBS.find(x => x.id === pending);
    if (v) enterVerb(v, '');
    uiStore.takePendingPaletteVerb(); // consume so we don't loop
  });

  function getIcon(name: string) {
    return (PLUGIN_ICONS[name] ?? Zap) as typeof Zap;
  }

  /** Highlight matching chars in a title */
  function highlightTitle(title: string, q: string): string {
    if (!q) return escHtml(title);
    const lq = q.toLowerCase();
    const lt = title.toLowerCase();
    if (lt.startsWith(lq)) {
      return `<span class="hl">${escHtml(title.slice(0, q.length))}</span>${escHtml(title.slice(q.length))}`;
    }
    // highlight first occurrence
    const idx = lt.indexOf(lq);
    if (idx !== -1) {
      return escHtml(title.slice(0, idx))
        + `<span class="hl">${escHtml(title.slice(idx, idx + q.length))}</span>`
        + escHtml(title.slice(idx + q.length));
    }
    return escHtml(title);
  }

  function escHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  // Placeholder flips between Phase 1 and Phase 2 so the user always knows
  // what the input will filter against.
  const TARGET_KIND_LABEL: Record<TargetKind, string> = {
    branch: 'branches', tag: 'tags', commit: 'commits',
    'project-file': 'project files',
    stash: 'stashes', remote: 'remotes', tab: 'open tabs', recent: 'recent repos', mr: 'merge requests',
    theme: 'themes',
    workspace: 'workspaces', 'ws-repo': 'repos in this workspace', 'ws-repo-any': 'repos in other workspaces',
    worktree: 'worktrees',
    'linear-issue': 'Linear issues', 'jira-issue': 'Jira issues',
  };
  const placeholder = $derived.by(() => {
    if (selectedVerb?.id === 'create-branch') {
      return createBranchStep === 'name'
        ? 'Branch name…  (Enter = from HEAD · Tab = pick parent)'
        : `Filter parent commits for '${pendingBranchName}'…  (Backspace to rename)`;
    }
    if (selectedVerb?.targetKind === 'linear-issue' || selectedVerb?.targetKind === 'jira-issue') {
      const which = selectedVerb.targetKind === 'jira-issue' ? 'Jira' : 'Linear';
      return `Search ${which} issues…  (# = code only, ~ = text only)`;
    }
    if (selectedVerb) return `Filter ${TARGET_KIND_LABEL[selectedVerb.targetKind]}…`;
    return "Type a command… (checkout, cherry-pick, stash, goto commit…)";
  });
</script>

<!-- Backdrop -->
<div
  class="palette-backdrop"
  role="presentation"
  onmousedown={(e) => { if (e.target === e.currentTarget) onClose(); }}
  transition:fade={{ duration: animStore.dBase }}
>
  <div class="palette-container" role="dialog" aria-modal="true" aria-label="Command Palette"
       transition:fly={{ y: -16, duration: animStore.dPanel, easing: cubicOut }}>

    <!-- Input row -->
    <div class="palette-header">
      <Search size={15} class="search-icon" />

      {#if selectedVerb}
        {@const VerbIcon = getIcon(selectedVerb.icon)}
        <button
          class="verb-chip"
          onclick={clearVerb}
          use:tooltip={{ content: 'Clear verb', shortcut: 'Backspace' }}
          aria-label="Clear {selectedVerb.title} verb"
        >
          <VerbIcon size={12} />
          <span class="verb-chip-label">{selectedVerb.title}</span>
          <ChevronRight size={12} class="verb-chip-arrow" />
        </button>
      {/if}

      <div class="input-ghost-wrapper">
        <input
          bind:this={inputEl}
          bind:value={query}
          onkeydown={onKeydown}
          {placeholder}
          autocomplete="off"
          spellcheck="false"
          class="palette-input"
        />
        {#if ghostSuffix}
          <!-- Ghost overlay positioned over the input -->
          <span class="ghost-overlay" aria-hidden="true">
            <span class="ghost-typed">{query}</span><span class="ghost-suffix">{ghostSuffix}</span>
          </span>
        {/if}
      </div>

      {#if ghostSuffix}
        <kbd class="tab-hint">Tab</kbd>
      {/if}
      <button class="mac-close-btn" onclick={onClose} use:tooltip={{ content: 'Close', shortcut: 'Esc' }} aria-label="Close"></button>
    </div>

    <!-- Results -->
    <div class="palette-results" bind:this={listEl}>
      {#if flatItems.length === 0 && targetsLoading}
        <div class="loading">
          <Spinner size="md" label={`Loading ${selectedVerb ? TARGET_KIND_LABEL[selectedVerb.targetKind] : 'data'}…`} />
        </div>
      {:else if flatItems.length === 0 && !loading}
        <div class="empty">
          {#if selectedVerb}
            {#if query.trim()}
              No {TARGET_KIND_LABEL[selectedVerb.targetKind]} matching <strong>{query.trim()}</strong>
            {:else if selectedVerb.targetKind === 'commit'}
              Type at least 2 characters to search commits
            {:else}
              No {TARGET_KIND_LABEL[selectedVerb.targetKind]} available
            {/if}
          {:else if query}
            No command matches <strong>{query}</strong>
          {:else}
            Pick a command to start
          {/if}
        </div>
      {:else}
        {#snippet renderItems()}
          {#each sections as section}
            <div class="section-header">{section.label}</div>
            {#each section.items as item}
              {@const idx = flatItems.indexOf(item)}
              {@const isSelected = idx === selectedIdx}
              {@const ItemIcon = getIcon(item.icon)}
              <button
                class="palette-item"
                class:selected={isSelected}
                data-idx={idx}
                onmouseenter={() => { selectedIdx = idx; }}
                onclick={() => item.action()}
                use:tooltip={item.subtitle ?? item.title}
              >
                <span class="item-icon">
                  <ItemIcon size={14} />
                </span>
                <span class="item-body">
                  <span class="item-title">
                    {@html highlightTitle(item.title, query)}
                  </span>
                  {#if item.subtitle}
                    <span class="item-subtitle">{item.subtitle}</span>
                  {/if}
                </span>
                {#if item.shortcut}
                  <span class="item-shortcut-slot"><Kbd label={item.shortcut} size="sm" tone="muted" /></span>
                {/if}
                {#if item.kind === 'verb'}
                  <span class="verb-marker" use:tooltip={'Select to pick a target'}>
                    <ChevronRight size={14} />
                  </span>
                {/if}
                {#if isSelected}
                  <span class="enter-hint">↵</span>
                {/if}
              </button>
            {/each}
          {/each}
        {/snippet}
        {@render renderItems()}
      {/if}
    </div>

    <!-- Footer -->
    <div class="palette-footer">
      <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
      <span><kbd>↵</kbd> {selectedVerb ? 'run' : 'pick command'}</span>
      {#if ghostSuffix}<span><kbd>Tab</kbd> complete</span>{/if}
      {#if selectedVerb}
        <span><kbd>⌫</kbd> clear verb</span>
      {:else}
        <span class="hint-muted">or type <kbd>verb</kbd> + space</span>
      {/if}
      <span><kbd>Esc</kbd> close</span>
    </div>
  </div>
</div>

{#if pendingConfirm}
  <ConfirmModal
    title={pendingConfirm.title}
    message={pendingConfirm.message}
    detail={pendingConfirm.detail}
    variant={pendingConfirm.variant ?? 'default'}
    confirmLabel={pendingConfirm.confirmLabel ?? 'Confirm'}
    onCancel={() => { pendingConfirm = null; onClose(); }}
    onConfirm={async () => {
      const p = pendingConfirm!;
      pendingConfirm = null;
      await p.onConfirm();
    }}
  />
{/if}

<style>
  /* ── Backdrop ───────────────────────────────────────────────────────────────── */

  .palette-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-menu);
    /* `backdrop-filter: blur()` removed — see Modal.svelte for rationale.
       Bumped the dim from 0.62 to 0.78 to compensate. */
    background: rgba(0, 0, 0, 0.78);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
  }

  /* ── Container ──────────────────────────────────────────────────────────────── */

  .palette-container {
    width: min(640px, 90vw);
    max-height: 70vh;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    /* Same premium shadow as MrModal */
    box-shadow: 0 32px 80px rgba(0, 0, 0, 0.7), 0 0 0 1px rgba(255, 255, 255, 0.04);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Header ─────────────────────────────────────────────────────────────────── */

  .palette-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  :global(.search-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  /* ── Verb chip (Phase 2 indicator) ─────────────────────────────────────── */
  .verb-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px 3px 9px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 999px;
    color: var(--accent);
    font: 600 12px/1 var(--font-ui-sans);
    letter-spacing: 0.01em;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--anim-dur-fast), border-color var(--anim-dur-fast);
    animation: verbChipIn var(--anim-dur-fast) ease-out;
  }

  .verb-chip:hover {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .verb-chip-label {
    white-space: nowrap;
  }

  :global(.verb-chip-arrow) {
    opacity: 0.7;
  }

  @keyframes verbChipIn {
    from { opacity: 0; transform: translateX(-6px); }
    to   { opacity: 1; transform: none; }
  }

  /* Ghost text wrapper — input + overlay stacked */
  .input-ghost-wrapper {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
  }

  .palette-input {
    width: 100%;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font: 13px/1.4 var(--font-ui-sans);
    caret-color: var(--accent);
    position: relative;
    z-index: 1;
  }

  .palette-input::placeholder {
    color: var(--text-disabled);
  }

  .ghost-overlay {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    font: 13px/1.4 var(--font-ui-sans);
    pointer-events: none;
    white-space: pre;
    z-index: 0;
  }

  .ghost-typed  { color: transparent; }
  .ghost-suffix { color: var(--text-disabled); }

  .tab-hint {
    font-size: 10px;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: var(--bg-base);
    flex-shrink: 0;
  }

  /* ── Results ────────────────────────────────────────────────────────────────── */

  .palette-results {
    flex: 1;
    overflow-y: auto;
    padding: 6px 0 4px;
    scroll-behavior: smooth;
  }

  .empty {
    padding: 32px 20px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .loading {
    padding: 32px 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .section-header {
    padding: 8px 16px 3px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-disabled);
    user-select: none;
  }

  .palette-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100% - 12px);
    margin: 1px 6px;
    padding: 6px 10px;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-size: 13px;
    font-family: var(--font-ui-sans);
    transition: background 80ms;
    border-radius: var(--radius-sm);
  }

  .palette-item:hover {
    background: var(--bg-hover);
  }

  /* Keyboard-selected item gets a subtle accent tint (same as MrModal selected file) */
  .palette-item.selected {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    outline: none;
  }

  .item-icon {
    color: var(--text-muted);
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  .palette-item.selected .item-icon { color: var(--accent); }

  .item-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .item-title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.item-title .hl) {
    color: var(--accent);
    font-weight: 600;
  }

  .item-subtitle {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-shortcut-slot {
    flex-shrink: 0;
    display: inline-flex;
  }

  .enter-hint {
    font-size: 12px;
    color: var(--text-muted);
    flex-shrink: 0;
    opacity: 0.7;
  }

  /* Chevron marker on verb items — signals "this will open a target picker" */
  .verb-marker {
    display: flex;
    align-items: center;
    color: var(--text-disabled);
    flex-shrink: 0;
    transition: color var(--anim-dur-fast), transform var(--anim-dur-fast);
  }

  .palette-item.selected .verb-marker {
    color: var(--accent);
    transform: translateX(2px);
  }

  /* ── Footer ─────────────────────────────────────────────────────────────────── */

  .palette-footer {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 7px 14px;
    border-top: 1px solid var(--border);
    background: var(--bg-elevated);
    font-size: 10px;
    color: var(--text-disabled);
    flex-shrink: 0;
  }

  .palette-footer kbd {
    font-size: 10px;
    padding: 1px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-base);
    color: var(--text-muted);
  }

  .palette-footer span { display: flex; align-items: center; gap: 4px; }

  .hint-muted kbd {
    opacity: 0.6;
  }
</style>
