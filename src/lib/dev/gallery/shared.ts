/** Gallery scene overrides and extra scenes for the "shared" area (shared/, plugins/, launcher/) — see gallery/registry.ts. */
import { createRawSnippet } from 'svelte';
import { GitBranch, Terminal, Search, Settings, Bell, Package, FolderTree, Bug } from 'lucide-svelte';
import type { SceneOverride, ExtraScene } from './scenes';

const noop = () => {};
const noopAsync = async () => {};

const raw = (html: string) => createRawSnippet(() => ({ render: () => html }));

const HOME = '/Users/dev';

const FINDING = {
  id: 'f-1',
  severity: 'high',
  state: 'detected',
  title: 'Prototype pollution in lodash.merge',
  description:
    'Versions of `lodash` before 4.17.21 are vulnerable to **prototype pollution** via `merge`, `mergeWith` and `defaultsDeep`.\n\nAn attacker controlling the merged object can add or modify properties of `Object.prototype`.',
  scanner: 'Dependabot',
  report_type: 'dependency_scanning',
  file_path: 'package.json',
  start_line: 42,
  web_url: 'https://github.com/example/app/security/dependabot/12',
  created_at: '2026-10-12T09:30:00Z',
  age_days: 20,
  identifiers: [
    { kind: 'CVE', value: 'CVE-2021-23337', url: 'https://nvd.nist.gov/vuln/detail/CVE-2021-23337' },
    { kind: 'GHSA', value: 'GHSA-35jh-r3h4-6jhm', url: null },
  ],
  provider: 'github',
  solution: 'Upgrade `lodash` to 4.17.21 or later.',
};

const railTabs = () => [
  {
    id: 'left',
    label: 'Left bar',
    hint: 'Drag to reorder, click the eye to hide.',
    sections: [
      {
        id: 'top',
        label: 'Top',
        hint: 'Main tools',
        items: [
          { id: 'files', label: 'Project', icon: FolderTree, visible: true, mandatory: true },
          { id: 'search', label: 'Search', icon: Search, visible: true, mandatory: false },
          { id: 'vcs', label: 'Git', icon: GitBranch, visible: true, mandatory: false },
          { id: 'debug', label: 'Debug', icon: Bug, visible: false, mandatory: false },
        ],
      },
      {
        id: 'bottom',
        label: 'Bottom',
        items: [
          { id: 'terminal', label: 'Terminal', icon: Terminal, visible: true, mandatory: false },
          { id: 'notifications', label: 'Notifications', icon: Bell, visible: true, mandatory: false },
          { id: 'plugins', label: 'Plugins', icon: Package, visible: false, mandatory: false },
          { id: 'settings', label: 'Settings', icon: Settings, visible: true, mandatory: true },
        ],
      },
    ],
  },
];

const formConfig = () => ({
  plugin_name: 'release-notes',
  title: 'Draft release notes',
  description: 'Collect the merged merge requests since the last tag and draft a changelog.',
  submit_label: 'Generate',
  submit_action: 'generate',
  width: '640px',
  height: '480px',
  nodes: [
    { type: 'text', name: 'from_tag', label: 'From tag', default: 'v1.3.0', placeholder: 'v1.2.0', required: true },
    { type: 'text', name: 'to_ref', label: 'To', default: 'HEAD' },
    { type: 'select', name: 'style', label: 'Style', default: 'conventional', options: [
      { value: 'conventional', label: 'Conventional commits' },
      { value: 'mr', label: 'Merge request titles' },
    ] },
    { type: 'toggle', name: 'group', label: 'Group by type', default: true },
    { type: 'checkbox', name: 'authors', label: 'Include authors', default: false },
    { type: 'alert', variant: 'info', text: 'Draft is written to RELEASE_NOTES.md in the repository root.' },
  ],
});

export const sharedOverrides: Record<string, SceneOverride> = {
  // ── shared/ ───────────────────────────────────────────────────────────────
  'shared/ConfirmModal': {
    props: {
      title: 'Delete branch?',
      message: 'The branch "feature/legacy-import" will be deleted from this repository.',
      detail: 'It has 3 commits that are not merged into main.',
      variant: 'danger',
      confirmLabel: 'Delete',
      onConfirm: noop,
      onCancel: noop,
    },
  },
  'shared/PromptModal': {
    props: {
      title: 'Rename branch',
      label: 'New branch name',
      initialValue: 'feature/legacy-import',
      placeholder: 'feature/…',
      confirmLabel: 'Rename',
      validate: (v: string) => (/\s/.test(v) ? 'Branch names cannot contain spaces' : null),
      onConfirm: noop,
      onCancel: noop,
    },
  },
  'shared/Modal': {
    props: {
      onClose: noop,
      width: '520px',
      height: 'auto',
      ariaLabel: 'Primitive modal',
      header: raw('<div style="padding:10px 14px;font-weight:600">Modal primitive</div>'),
      children: raw('<div style="padding:16px;color:var(--text-secondary)">Body content goes here.</div>'),
    },
  },
  'shared/CustomizeRailsModal': {
    props: () => ({ tabs: railTabs(), onSave: noopAsync, onResetTab: () => railTabs()[0].sections, onClose: noop }),
  },
  'shared/DeepLinkConfirmModal': {
    props: {
      url: 'https://github.com/nightprint-studio/arbor.git',
      actionDescription: 'Open commit 3fa91c2',
      reason: 'unknown',
      onClose: noop,
      onConfirmed: noop,
    },
  },
  'shared/DeepLinkActionConfirmModal': {
    props: {
      title: 'Open merge request !123',
      description: 'Arbor will check out the source branch and show the MR.',
      url: 'https://github.com/nightprint-studio/arbor.git',
      onConfirm: noop,
      onCancel: noop,
    },
  },
  'shared/DeepLinkDisabledModal': {
    props: {
      title: 'Deep links are disabled',
      message: 'Opening links from the browser is turned off in Settings → General.',
      url: 'arbor://open?url=https://github.com/nightprint-studio/arbor.git',
      onClose: noop,
    },
  },
  'shared/DeepLinkLoadingModal': {
    props: { title: 'Opening merge request !123', status: 'loading', onClose: noop },
  },
  'shared/ExternalLinkConfirmModal': {
    props: { url: 'vscode://file/Users/dev/dev/arbor/src/main.rs:42', scheme: 'vscode', onConfirm: noop, onCancel: noop },
  },
  'shared/internal/FilePropertiesModal': { props: { path: `${HOME}/dev/arbor/src`, onClose: noop } },
  'shared/internal/stage/DiscardConfirmModal': { props: { target: 'all 5 files', onConfirm: noop, onCancel: noop } },
  'shared/internal/security/SecurityFindingDetailModal': { props: { finding: FINDING, onClose: noop } },
  'shared/ActiveSchedulesModal': { props: { onClose: noop } },
  'shared/ProcessMonitorModal': { props: { onClose: noop }, settle: 1500 },
  'shared/ProfileManagerModal': { props: { onClose: noop } },
  'shared/RecentReposModal': { props: { onClose: noop } },
  'shared/ShortcutsModal': { props: { onClose: noop } },
  'shared/ThemeEditorModal': { props: { onClose: noop } },
  'shared/McpActivityModal': {
    props: { onClose: noop },
    setup: async () => (await import('$lib/stores/mcp.svelte')).mcpStore.refreshAudit(),
  },
  'shared/McpToolsModal': { props: { onClose: noop } },
  'shared/McpProjectRuleModal': { props: { root: `${HOME}/dev/arbor`, name: 'arbor', product: 'bennu', onClose: noop } },
  'shared/McpConsentModal': {
    setup: async () => {
      const { mcpStore } = await import('$lib/stores/mcp.svelte');
      mcpStore.enqueue({
        id: 'req-1',
        tool: 'bennu_write_file',
        title: 'Write file',
        program: 'bennu',
        safety: 'write',
        description: 'Write a file in a project, keeping its original encoding.',
        arguments: JSON.stringify({ path: 'src/main/java/com/acme/OrderService.java', content: '…(2.1 KB)' }, null, 2),
      });
      mcpStore.enqueue({
        id: 'req-2', tool: 'bennu_test_run', title: 'Run tests', program: 'bennu', safety: 'destructive',
        description: 'Run the project tests.', arguments: '{}',
      });
    },
  },
  'shared/CredentialsModal': {
    setup: async () => (await import('$lib/stores/credentials.svelte')).credentialsStore.show(),
  },
  'shared/OnboardingModal': {
    setup: async () => (await import('$lib/stores/onboarding.svelte')).onboardingStore.show(),
  },
  'shared/MarkdownEditorModal': {
    setup: async () =>
      (await import('$lib/stores/markdown.svelte')).markdownStore.openFile({ path: `${HOME}/dev/arbor/README.md` }),
  },

  // ── plugins/ ──────────────────────────────────────────────────────────────
  'plugins/manager/PluginDepGraphModal': { props: { onClose: noop } },
  'plugins/manager/PluginDisableConfirmModal': {
    props: { pluginName: 'source-export', cascade: ['release-notes', 'source-export'], onConfirm: noop, onCancel: noop },
  },
  'plugins/manager/PluginEnableConfirmModal': {
    props: { pluginName: 'release-notes', cascade: ['source-export', 'release-notes'], blockers: [], onConfirm: noop, onCancel: noop },
  },
  'plugins/manager/PluginUninstallConfirmModal': {
    props: { pluginName: 'release-notes', dependents: ['changelog-pro'], onConfirm: noop, onCancel: noop },
  },
  'plugins/manager/PluginInfoModal': { props: { pluginName: 'release-notes', onClose: noop, onOpenSettings: noop } },
  'plugins/manager/PluginExportTemplateModal': { props: { onClose: noop } },
  'plugins/marketplace/MarketplaceAddCustomSourceModal': { props: { onResolved: noop, onClose: noop } },
  'plugins/MarketplaceModal': { props: { onClose: noop }, settle: 900 },
  'plugins/PluginFormModal': { props: () => ({ form: formConfig(), onClose: noop }) },
  'plugins/ContributableModal': {
    props: { containerId: 'release-notes::settings', onClose: noop },
    setup: async () => {
      const { containerStore } = await import('$lib/stores/corvus/container.svelte');
      const { contributionStore } = await import('$lib/stores/corvus/contribution.svelte');
      await containerStore.reloadDefs();
      await contributionStore.reloadAll();
    },
    settle: 800,
  },

  // ── launcher/ ─────────────────────────────────────────────────────────────
  'launcher/mcp/McpSettingsModal': {
    props: { onClose: noop },
    setup: async () => (await import('$lib/stores/mcp.svelte')).mcpStore.load(),
    settle: 700,
  },
};

const overlay = () => import('./OverlayScene.svelte');

export const sharedExtras: ExtraScene[] = [
  { id: 'shared/WidgetCatalog', load: () => import('./WidgetCatalog.svelte'), settle: 600 },
  { id: 'shared/ContextMenu', load: overlay, props: { kind: 'context-menu' } },
  { id: 'shared/Dropdown', load: overlay, props: { kind: 'dropdown' }, settle: 600 },
  { id: 'shared/DropdownSearchable', load: overlay, props: { kind: 'dropdown-searchable' }, settle: 600 },
  { id: 'shared/Popover', load: overlay, props: { kind: 'popover' } },
  {
    id: 'shared/ToastStack',
    load: () => import('$lib/feedback/FeedbackHost.svelte'),
    setup: async () => {
      const { toastStore } = await import('$lib/feedback/stores/toasts.svelte');
      toastStore.show('Fetched 3 remotes', 'success', 0);
      toastStore.show('Push rejected: the remote contains work you do not have locally.', 'error', 0, [
        { label: 'Pull', onClick: () => {} },
        { label: 'Force push', onClick: () => {} },
      ]);
      toastStore.show('Branch policy: names must start with feature/ or fix/', 'warning', 0);
      toastStore.show('Index ready', 'info', 0);
    },
    settle: 900,
  },
  {
    id: 'shared/CommandPalette',
    load: () => import('$lib/components/shared/CommandPalette.svelte'),
    props: { onClose: noop },
    settle: 800,
  },
];
