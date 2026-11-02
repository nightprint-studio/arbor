/** Gallery scene overrides and extra scenes for the "corvus" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';
import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
import { repoStore } from '$lib/stores/corvus/repo.svelte';
import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
import { pipelinesStore } from '$lib/stores/corvus/pipelines.svelte';
import { uiStore } from '$lib/stores/ui.svelte';
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
  'corvus/mr/MrModal': withTab({ props: { mr, onClose: noop, onRefresh: noop }, settle: 800 }),
  'corvus/pipeline/CiPipelineDetailModal': withTab({ props: { run: ciRun, tabId: TAB_ID, onClose: noop, onRetrigger: noop } }),
  'corvus/pipeline/CreateCiPipelineModal': withTab({ props: { provider: ciProvider, tabId: TAB_ID, onClose: noop, onCreated: noop } }),
  'corvus/security/SecurityDetailModal': withTab({ props: { tabId: TAB_ID, summary: securitySummary, initialSeverity: 'high', onClose: noop }, settle: 800, setup: () => securityStore.loadSummary(TAB_ID) }),
  'corvus/sidebar/BranchRenameModal': withTab({ props: { branch: branches[1], onClose: noop, onRenamed: noop } }),
  'corvus/sidebar/RemoteBranchRenameModal': withTab({ props: { branch: remoteBranches[2], onClose: noop, onRenamed: noop } }),
  'corvus/sidebar/SubmoduleBranchModal': withTab({ props: { sub: submodule, onClose: noop, onDone: noop } }),
  'corvus/sidebar/WorktreeInfoModal': withTab({ props: { worktree, onClose: noop, onSwitch: noop, onOpenInIde: noop } }),
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
  { id: 'corvus/stage/CommitForm', load: () => import('$lib/components/corvus/stage/CommitForm.svelte'), setup: seedTab, props: {} },
  { id: 'corvus/security/SecurityPanel', load: () => import('$lib/components/corvus/security/SecurityPanel.svelte'), setup: seedTab, props: {} },
  { id: 'corvus/BranchSwitchPopup', load: () => import('$lib/components/corvus/BranchSwitchPopup.svelte'), props: { x: 300, y: 200, branches: [{ name: 'main', is_head: false }, { name: 'feature/bennu', is_head: true }], onSelect: noop, onClose: noop } },
];

