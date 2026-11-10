/** Gallery scene overrides and extra scenes for the "corvus" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';
import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
import { repoStore } from '$lib/stores/corvus/repo.svelte';
import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
import { pipelinesStore } from '$lib/stores/corvus/pipelines.svelte';
import { uiStore } from '$lib/stores/ui.svelte';
import { plansStore } from '$lib/stores/corvus/plans.svelte';
import { corvusImberSchedules } from '$lib/stores/corvus/imber-schedules.svelte';
import type { ImberAction } from '$lib/types/corvus/imber';
import type { ImberJob } from '$lib/types/imber-schedule';
import type { Plan } from '$lib/types/corvus/plan';
import { imberStore } from '$lib/stores/corvus/imber.svelte';
import { securityStore } from '$lib/stores/corvus/security.svelte';
import {
  TAB_ID, REPO_PATH, branches, remoteBranches, commitNode, worktree, submodule, mr, ciRun, ciProvider,
  securitySummary, issues, workspaceDef,
} from '../fixtures/corvus';

const noop = () => {};

/** Console helpers for the UI review: `__audit()` summarises one open modal, `__focus()` describes the focused element. */
if (typeof window !== 'undefined') {
  const w = window as unknown as Record<string, unknown>;
  const rect = (e: Element) => e.getBoundingClientRect();
  const vis = (e: Element) => { const b = rect(e); return b.width > 0 && b.height > 0 && getComputedStyle(e).visibility !== 'hidden'; };
  const desc = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.split(' ').filter((c) => c && !c.startsWith('s-') && !c.startsWith('svelte-')).slice(0, 2).join('.') : '';
    return `${e.tagName.toLowerCase()}${cls ? '.' + cls : ''}:${(e.getAttribute('aria-label') || (e as HTMLElement).title || e.textContent || '').trim().slice(0, 28)}`;
  };
  w.__audit = () => {
    const root = document.querySelector('[role=dialog]') ?? document.body;
    const q = (sel: string) => [...root.querySelectorAll(sel)].filter(vis);
    const controls = q('button,[role=button],a[href],input,select,textarea');
    return JSON.stringify({
      dialog: desc(root), size: [Math.round(rect(root).width), Math.round(rect(root).height)],
      small: controls.filter((b) => rect(b).height < 22 || rect(b).width < 22).map((b) => `${desc(b)} ${Math.round(rect(b).width)}x${Math.round(rect(b).height)}`).slice(0, 10),
      noname: controls.filter((b) => b.tagName !== 'INPUT' && !(b.getAttribute('aria-label') || (b as HTMLElement).title || b.textContent?.trim())).map(desc).slice(0, 8),
      clipped: q('*').filter((e) => e.children.length === 0 && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflow !== 'visible' && getComputedStyle(e).textOverflow !== 'ellipsis' && e.textContent?.trim()).map(desc).slice(0, 8),
      ellipsized: q('*').filter((e) => e.children.length === 0 && e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).textOverflow === 'ellipsis').map(desc).slice(0, 6),
      fonts: [...new Set(q('*').filter((e) => e.children.length === 0 && e.textContent?.trim()).map((e) => getComputedStyle(e).fontSize))].join(','),
      nativeCheckboxes: q('input[type=checkbox]').length, rawButtons: q('button').length,
    });
  };
  w.__focus = () => {
    const e = document.activeElement; if (!e || e === document.body) return 'body';
    const cs = getComputedStyle(e);
    return `${desc(e)} outline=${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor} shadow=${cs.boxShadow.slice(0, 60)}`;
  };
}

/** One open repository tab, so every modal that reads `tabsStore.activeTab` has a repo to talk to. */
function seedTab(): void {
  if (!tabsStore.tabs.some((t) => t.id === TAB_ID)) {
    tabsStore.addTab({
      tab_id: TAB_ID, path: REPO_PATH, name: 'arbor', current_branch: 'feature/bennu', is_bare: false, is_empty: false,
    });
  }
  tabsStore.setActive(TAB_ID);
  repoStore.setLocalBranches(branches as never);
  repoStore.setRemoteBranches(remoteBranches as never);
}

/** Schedules for the Imber Schedules window: a repeat that ended well, a one-time run waiting, a failed one, one that is off. */
const NOW = Date.now();
const imberJobs: ImberJob[] = [
  { id: 's1', name: 'Morning check', enabled: true, recurrence: { kind: 'weekly', days: [0, 1, 2, 3, 4], hour: 9, minute: 0 }, missed: 'skip',
    action: { kind: 'awake', conversation_id: 'c-1', prompt: 'What changed on main since yesterday? Summarise it.' }, root: REPO_PATH, mode: 'manual',
    created_ms: NOW - 9 * 86_400_000, next_run_ms: NOW + 14 * 3_600_000, last_run: { at_ms: NOW - 10 * 3_600_000, ok: true, detail: '3 new commits on main, all from Marta: the coupon rounding fix and two docs changes.' }, runs: 7 },
  { id: 's2', name: 'Fetch every 30 minutes', enabled: true, recurrence: { kind: 'interval', secs: 1800, anchor_ms: NOW - 3_600_000 }, missed: 'skip',
    action: { kind: 'fetch', remote: null }, root: REPO_PATH, mode: 'manual', created_ms: NOW - 3_600_000, next_run_ms: NOW + 12 * 60_000,
    last_run: { at_ms: NOW - 18 * 60_000, ok: false, detail: 'Authentication failed for origin' }, runs: 2 },
  { id: 's3', name: 'Release reminder', enabled: true, recurrence: { kind: 'once', at_ms: NOW + 3 * 86_400_000 }, missed: 'run_once',
    action: { kind: 'prompt', prompt: 'List the commits since the last tag and draft the release notes.' }, root: REPO_PATH, mode: 'auto',
    created_ms: NOW, next_run_ms: NOW + 3 * 86_400_000, last_run: null, runs: 0 },
  { id: 's4', name: 'Friday digest', enabled: false, recurrence: { kind: 'cron', expr: '0 0 17 * * 5' }, missed: 'skip',
    action: { kind: 'prompt', prompt: 'Summarise this week’s merged work.' }, root: REPO_PATH, mode: 'manual', created_ms: NOW - 20 * 86_400_000, next_run_ms: null,
    last_run: { at_ms: NOW - 4 * 86_400_000, ok: true, detail: 'Five merge requests landed this week.' }, runs: 3 },
  { id: 's5', name: 'Nightly fetch', enabled: true, recurrence: { kind: 'daily', hour: 2, minute: 30 }, missed: 'skip',
    action: { kind: 'fetch', remote: 'origin' }, root: REPO_PATH, mode: 'manual', created_ms: NOW - 6 * 86_400_000, next_run_ms: NOW + 9 * 3_600_000,
    last_run: { at_ms: NOW - 12 * 3_600_000, ok: true, skipped: true, detail: 'Missed while Corvus was closed.' }, runs: 5 },
];

/** A plan Imber built and started: one phase done, the MR open, the next phase waiting for it to be merged, a failed step to retry. */
const imberPlan: Plan = {
  id: 'plan-imber', name: 'Release 0.9 across api and web', status: 'paused', pause_reason: 'a step failed', template: false, created_ms: NOW - 600_000, updated_ms: NOW - 60_000,
  phases: [
    { id: 'ph1', name: 'Phase 1', gate: { kind: 'phase_done', state: 'open' }, steps: [
      { id: 'st1', kind: 'push', branch: 'release/0.9', repo: { path: REPO_PATH, name: 'api' }, status: 'done' },
      { id: 'mr1', kind: 'create_mr', source: 'release/0.9', target: 'main', title: 'Release 0.9', description: '', labels: [], draft: false, auto_merge: false, squash: false, delete_branch: false, repo: { path: REPO_PATH, name: 'api' }, status: 'done', result: { number: 118 } },
    ] },
    { id: 'ph2', name: 'Phase 2', gate: { kind: 'mr_merged', step_id: 'mr1', poll_secs: 30, timeout_secs: 7200, state: 'waiting' }, steps: [
      { id: 'st3', kind: 'push', branch: 'release/0.9', repo: { path: `${REPO_PATH}-web`, name: 'web' }, status: 'failed', error: 'rejected: the remote is ahead of release/0.9' },
      { id: 'st4', kind: 'pull', repo: { path: `${REPO_PATH}-web`, name: 'web' }, status: 'held' },
    ] },
  ],
  finally: [{ id: 'st5', kind: 'tag', name: 'v0.9.0', repo: { path: REPO_PATH, name: 'api' }, status: 'queued' }],
};

/** What Imber did here: a commit and a branch that can be undone (the commit first), a deleted branch already restored, a push that cannot. */
const imberActionLog: ImberAction[] = [
  { id: 'a5', at_ms: NOW - 120_000, root: REPO_PATH, tool: 'git_commit', summary: 'Committed “fix: round coupon totals”', undo: { op: 'reset_soft' }, irreversible: null, undone_ms: null },
  { id: 'a4', at_ms: NOW - 400_000, root: REPO_PATH, tool: 'git_push', summary: 'Pushed to origin', undo: null, irreversible: 'It was pushed to origin: others may already have it. Undoing it would need a force push, which Imber does not do on its own.', undone_ms: null },
  { id: 'a3', at_ms: NOW - 900_000, root: REPO_PATH, tool: 'git_branch_create', summary: 'Created branch fix/coupon-totals', undo: { op: 'delete_branch' }, irreversible: null, undone_ms: null },
  { id: 'a2', at_ms: NOW - 3_000_000, root: REPO_PATH, tool: 'git_branch_delete', summary: 'Deleted branch old/experiment', undo: { op: 'recreate_branch' }, irreversible: null, undone_ms: NOW - 2_900_000 },
  { id: 'a1', at_ms: NOW - 7_000_000, root: REPO_PATH, tool: 'git_mr_create', summary: 'Opened merge request “Release 0.8”', undo: null, irreversible: 'The merge request was opened on the hosting provider. Close it there.', undone_ms: null },
];

const withTab = (o: SceneOverride): SceneOverride => ({ ...o, setup: async () => { seedTab(); await o.setup?.(); } });

export const corvusOverrides: Record<string, SceneOverride> = {
  'corvus/DependencyTreeModal': withTab({
    props: {
      pluginName: 'cargo-explorer', providerAction: 'show_dependencies', title: 'Dependencies',
      node: { id: 'crate:bennu-check', label: 'bennu-check', icon: 'package', kind: 'module', selectable: true, expanded: false, badge: '0.1.0' },
      onClose: noop,
    },
  }),
  'corvus/GitBlameModal': withTab({ props: { tabId: TAB_ID, path: 'crates/products/bennu/project/src/tree.rs', onClose: noop }, settle: 800 }),
  'corvus/graph/CreateBranchModal': withTab({ props: { node: commitNode, tabId: TAB_ID, onClose: noop, onCreate: noop } }),
  'corvus/graph/CreateTagModal': withTab({ props: { node: commitNode, onClose: noop, onCreate: noop } }),
  'corvus/issues/IssueDetailModal': withTab({ props: { issue: issues[0], provider: 'linear', onClose: noop } }),
  'corvus/mr/CreateMrModal': withTab({
    props: { onClose: noop, onCreated: noop, currentBranch: 'feature/bennu', initialTargetBranch: 'main', initialTitle: 'gradle support & attachment on Imber chat' },
  }),
  'corvus/imber/ImberActionsModal': withTab({ props: { onClose: noop }, setup: () => imberStore.seedActions(imberActionLog) }),
  'shared/ImberSchedulesModal': withTab({ props: { store: corvusImberSchedules, onClose: noop }, setup: () => corvusImberSchedules.seed(imberJobs) }),
  'corvus/plan/PlanModal': withTab({
    settle: 900,
    setup: async () => { await plansStore.load(); plansStore.openModal('plan-release'); },
  }),
  'corvus/mr/MrModal': withTab({ props: { mr, onClose: noop, onRefresh: noop }, settle: 800 }),
  'corvus/pipeline/CiPipelineDetailModal': withTab({ props: { run: ciRun, tabId: TAB_ID, onClose: noop, onRetrigger: noop } }),
  'corvus/pipeline/CreateCiPipelineModal': withTab({ props: { provider: ciProvider, tabId: TAB_ID, onClose: noop, onCreated: noop } }),
  'corvus/security/SecurityDetailModal': withTab({ props: { tabId: TAB_ID, summary: securitySummary, initialSeverity: 'high', onClose: noop }, settle: 800, setup: () => securityStore.loadSummary(TAB_ID) }),
  'corvus/sidebar/BranchRenameModal': withTab({ props: { branch: branches[1], onClose: noop, onRenamed: noop } }),
  'corvus/sidebar/RemoteBranchRenameModal': withTab({ props: { branch: remoteBranches[2], onClose: noop, onRenamed: noop } }),
  'corvus/sidebar/SubmoduleBranchModal': withTab({ props: { sub: submodule, onClose: noop, onDone: noop } }),
  'corvus/sidebar/WorktreeInfoModal': withTab({ props: { worktree, onClose: noop, onSwitch: noop, onOpenInIde: noop } }),
  'corvus/OpenUnregisteredRepoModal': withTab({ props: { path: 'C:\\Dev\\side-project', name: 'side-project' }, setup: () => workspacesStore.load() }),
  'corvus/TicketPickerModal': withTab({ props: { onSelect: noop, onClose: noop }, settle: 800 }),
  'corvus/workspace/CreateWorkspaceModal': withTab({ props: { onClose: noop }, setup: () => workspacesStore.load() }),
  'corvus/workspace/WorkspaceManagementModal': withTab({ props: { onClose: noop, onCreate: noop }, setup: () => workspacesStore.load() }),
  'corvus/workspace/WorkspaceTagAllModal': withTab({ props: { workspace: workspaceDef, onClose: noop }, settle: 800, setup: () => workspacesStore.load() }),
  'corvus/sidebar/BranchCompareModal': withTab({ props: { fromRef: 'main', toRef: 'feature/bennu', onClose: noop } }),
  'corvus/graph/NotesModal': withTab({ props: { commitOid: commitNode.oid, shortOid: commitNode.short_oid, onClose: noop } }),
  'corvus/InitRepoModal': withTab({ props: { path: '/Users/christian/sviluppo/mio/workspace/rust/geode-audio', onInit: noop, onCancel: noop } }),
  'corvus/sidebar/DeleteRemoteBranchModal': withTab({ props: { branchName: 'origin/release/0.9', onConfirm: noop, onCancel: noop } }),
  'corvus/sidebar/DeleteTagModal': withTab({ props: { tagName: 'v0.8.2', scope: 'remote', onConfirm: noop, onCancel: noop } }),
  'corvus/sidebar/AddWorktreeModal': withTab({ props: { initialBranch: 'fix/context-menu-flicker', onClose: noop, onAdded: noop } }),
  // These three render nothing until a uiStore flag opens them (they are mounted once in AppShell).
  'corvus/RecentReposModal': withTab({ props: { onOpen: noop }, setup: () => uiStore.setRecentQuickSwitchOpen(true) }),
  'corvus/linked-worktrees/WorktreeLinkManagerModal': withTab({ setup: async () => { await workspacesStore.load(); uiStore.openLinkManager(null); } }),
  'corvus/linked-worktrees/AddToWorktreeLinkModal': withTab({ setup: () => uiStore.openAddToLink('r1') }),
  'corvus/pipeline/PipelineRunDetailModal': withTab({ setup: async () => { await pipelinesStore.reload(); pipelinesStore.setActiveRun('run-7f3a'); } }),
  'corvus/DepsExplorerModal': { skip: 'Opens only when a plugin pushes a dependency-tree snapshot for its sidebar id (contribution events); not reproducible with static props.' },
  'corvus/RepoBrowserModal': withTab({ props: { onClose: noop }, settle: 800 }),
};
// Every other corvus modal still gets a repo tab, so the ones that read the active tab are not empty.
const allIds = Object.keys(import.meta.glob('/src/lib/components/corvus/**/*Modal.svelte'))
  .map((p) => p.slice('/src/lib/components/'.length).replace(/\.svelte$/, ''));
for (const id of allIds) corvusOverrides[id] ??= withTab({});

const stashes = [
  { index: 0, message: 'WIP on feature/bennu: gradle importer', oid: 'b0b1b2b3c4c5' },
  { index: 1, message: 'On develop: experiment with lane colors', oid: 'd4d5d6d7e8e9' },
];

export const corvusExtras: ExtraScene[] = [
  { id: 'corvus/sidebar/BranchTree[local]', load: () => import('$lib/components/corvus/sidebar/BranchTree.svelte'), setup: seedTab, props: { branches, type: 'local' } },
  { id: 'corvus/sidebar/BranchTree[remote]', load: () => import('$lib/components/corvus/sidebar/BranchTree.svelte'), setup: seedTab, props: { branches: remoteBranches, type: 'remote' } },
  { id: 'corvus/sidebar/StashList', load: () => import('$lib/components/corvus/sidebar/StashList.svelte'), setup: seedTab, props: { stashes, onRefresh: noop } },
  { id: 'corvus/sidebar/SubmoduleList', load: () => import('$lib/components/corvus/sidebar/SubmoduleList.svelte'), setup: seedTab, props: { submodules: [submodule, { ...submodule, name: 'vendor/openssl-src', path: 'vendor/openssl-src', branch: null, behind: 0, is_dirty: true }], onRefresh: noop } },
  { id: 'corvus/mr/MrSidebar', load: () => import('$lib/components/corvus/mr/MrSidebar.svelte'), setup: seedTab, props: { onOpenCreate: noop, onOpenDetail: noop } },
  { id: 'corvus/plan/PlansTab', load: () => import('$lib/components/corvus/plan/PlansTab.svelte'), setup: async () => { seedTab(); await plansStore.load(); }, props: {}, settle: 600 },
  { id: 'corvus/mr/CreateMrModal[plan]', load: () => import('$lib/components/corvus/mr/CreateMrModal.svelte'), setup: seedTab, props: { planMode: true, onClose: noop, onCreated: noop, onPlan: noop, currentBranch: 'feature/bennu', initialTargetBranch: 'main', initialTitle: 'Add OAuth2 PKCE flow' } },
  { id: 'corvus/imber/ImberPlanCard', load: () => import('$lib/components/corvus/imber/ImberPlanCard.svelte'), setup: seedTab, props: { snapshot: imberPlan } },
  { id: 'shared/ImberSchedulesModal[corvus new]', load: () => import('$lib/components/shared/ImberSchedulesModal.svelte'), setup: () => { seedTab(); corvusImberSchedules.seed(imberJobs); corvusImberSchedules.edit('new'); }, props: { store: corvusImberSchedules, onClose: noop } },
  { id: 'corvus/stage/CommitForm', load: () => import('$lib/components/corvus/stage/CommitForm.svelte'), setup: seedTab, props: {} },
  { id: 'corvus/security/SecurityPanel', load: () => import('$lib/components/corvus/security/SecurityPanel.svelte'), setup: seedTab, props: {} },
  { id: 'corvus/BranchSwitchPopup', load: () => import('$lib/components/corvus/BranchSwitchPopup.svelte'), props: { x: 300, y: 200, branches: [{ name: 'main', is_head: false }, { name: 'feature/bennu', is_head: true }], onSelect: noop, onClose: noop } },
];

