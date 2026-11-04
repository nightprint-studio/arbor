/** Mock answers for the "shared" area (shared/, plugins/, launcher/) — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const NOW = Math.floor(Date.now() / 1000);
const GB = 1024 ** 3;
const MB = 1024 ** 2;

const perms = (over: Record<string, unknown> = {}) => ({
  network: [], fs: 'read', fs_scope: [], git: 'read', terminal: 'none', terminal_scope: [],
  env_read: false, issues: 'none', provider: 'none', toolchain: 'none', ...over,
});

const dep = (name: string, version: string, optional = false) => ({ name, version, optional });

/** The plugins the manager, dep-graph and enable/disable modals draw from. */
export const MOCK_PLUGINS = [
  {
    name: 'source-export', version: '1.4.0', description: 'Export a project tree as a single annotated text file for AI context.',
    author: 'nightprint-studio', license: 'MIT', repository: 'https://github.com/nightprint-studio/arbor-extensions',
    keywords: ['export', 'ai'], arbor_api: 3, enabled: true, permissions: perms({ fs: 'write' }),
    hooks: { on_repo_open: true }, scheduler_count: 0, schedulers_running: 0, schedules: [],
    dependencies: [], required_by: ['release-notes'],
  },
  {
    name: 'release-notes', version: '0.9.2', description: 'Draft release notes from merged MRs and conventional commits.',
    author: 'nightprint-studio', license: 'MIT', keywords: ['release'], arbor_api: 3, enabled: true,
    permissions: perms({ git: 'write', provider: 'read', network: ['api.github.com'] }),
    hooks: { on_commit: true, on_push: true }, scheduler_count: 1, schedulers_running: 1,
    schedules: [{ action: 'refresh_drafts', trigger: { kind: 'fixed_rate', interval_sec: 900 }, initial_delay_sec: 30, on_load: true, only_when_focused: true, running: true }],
    dependencies: [dep('source-export', '>=1.0.0'), dep('git-extras', '>=0.3.0', true)], required_by: [],
  },
  {
    name: 'encoding-guardian', version: '2.1.1', description: 'Warns before committing files whose encoding would be corrupted.',
    author: 'christian', arbor_api: 3, enabled: true, experimental: true, permissions: perms({ fs: 'read' }),
    hooks: { on_commit: true }, scheduler_count: 0, schedulers_running: 0, schedules: [], dependencies: [], required_by: [],
  },
  {
    name: 'bevy-brp', version: '0.3.0', description: 'Bevy Remote Protocol inspector for running games.',
    author: 'nightprint-studio', arbor_api: 3, enabled: false, permissions: perms({ network: ['127.0.0.1'] }),
    hooks: {}, scheduler_count: 0, schedulers_running: 0, schedules: [], dependencies: [dep('missing-helper', '>=2.0.0')],
    dep_error: 'dependency "missing-helper" (>=2.0.0) is not installed', required_by: [],
  },
];

const edge = (name: string, version: string, optional = false, unmet = false) => ({ name, version, optional, unmet });

const MOCK_THEME_PREVIEW = { bg: '#1e1f22', fg: '#dfe1e5', accent: '#6ea0ff', success: '#5fb865', warning: '#e0a33c', error: '#e05555' };

const marketplacePlugins = [
  {
    name: 'source-export', version: '1.4.0', description: 'Export a project tree as a single annotated text file for AI context.',
    author: 'nightprint-studio', category: 'ai', tags: ['export', 'ai'], source: 'official', installed: true, enabled: true,
    installed_version: '1.4.0', entry: { repo: 'https://github.com/nightprint-studio/arbor-extensions', subpath: 'plugins/source-export', source: 'official' },
    permissions: perms({ fs: 'write' }),
  },
  {
    name: 'release-notes', version: '1.0.0', description: 'Draft release notes from merged MRs and conventional commits.',
    author: 'nightprint-studio', category: 'git-workflow', tags: ['release', 'git'], source: 'official', installed: true, enabled: true,
    installed_version: '0.9.2', update_available: '1.0.0', entry: { repo: 'https://github.com/nightprint-studio/arbor-extensions', subpath: 'plugins/release-notes', source: 'official' },
    permissions: perms({ git: 'write', provider: 'read', network: ['api.github.com'] }),
    dependencies: [dep('source-export', '>=1.0.0')],
  },
  {
    name: 'docker-compose', version: '0.5.1', description: 'Start, stop and tail docker compose stacks from the sidebar.',
    author: 'community-dev', category: 'build', tags: ['docker', 'devops'], source: 'community', installed: false,
    entry: { repo: 'https://github.com/community-dev/arbor-docker', source: 'community', external: true },
    permissions: perms({ terminal: 'commands', terminal_scope: ['docker'] }),
  },
  {
    name: 'jira-bridge', version: '2.0.0', description: 'Link commits to Jira issues and transition them on merge.',
    author: 'acme-corp', category: 'ci', tags: ['jira', 'issues'], source: 'community', installed: false, experimental: true,
    entry: { repo: 'https://github.com/acme-corp/arbor-jira', source: 'community', external: true, pinned_sha: 'a1b2c3d' },
    permissions: perms({ issues: 'write', network: ['*.atlassian.net'] }),
  },
  {
    name: 'my-local-tools', version: '0.1.0', description: 'Sideloaded scratch plugin.',
    author: 'christian', source: 'local', installed: true, enabled: false,
    entry: { repo: '', source: 'local' },
    permissions: perms(),
  },
];

const marketplaceThemes = [
  { id: 'nord-night', name: 'Nord Night', description: 'Cold, low-contrast blues.', author: 'community-dev', tags: ['dark'], preview: { ...MOCK_THEME_PREVIEW, bg: '#2e3440', accent: '#88c0d0' }, variant: 'dark', source: 'community', installed: true, entry: { repo: 'https://github.com/community-dev/arbor-nord', source: 'community', external: true } },
  { id: 'solar-paper', name: 'Solar Paper', description: 'Warm light theme.', author: 'nightprint-studio', tags: ['light'], preview: { bg: '#fdf6e3', fg: '#073642', accent: '#268bd2', success: '#859900', warning: '#b58900', error: '#dc322f' }, variant: 'light', source: 'official', installed: false, entry: { repo: 'https://github.com/nightprint-studio/arbor-extensions', source: 'official' } },
  { id: 'dracula', name: 'Dracula', description: 'The classic purple-tinted dark theme.', author: 'community-dev', tags: ['dark'], preview: { ...MOCK_THEME_PREVIEW, bg: '#282a36', accent: '#bd93f9' }, variant: 'dark', source: 'community', installed: false, entry: { repo: 'https://github.com/community-dev/arbor-dracula', source: 'community', external: true } },
];

const d = (secs: number) => ({ secs, nanos: 0 });

export const sharedFixtures: Record<string, FixtureHandler> = {
  // ── Plugins ───────────────────────────────────────────────────────────────
  'corvus.list_plugin_info': MOCK_PLUGINS,
  'corvus.plugin_dep_graph': [
    { name: 'source-export', version: '1.4.0', enabled: true, depends_on: [], dependents: [edge('release-notes', '>=1.0.0')], error: null },
    { name: 'release-notes', version: '0.9.2', enabled: true, depends_on: [edge('source-export', '>=1.0.0'), edge('git-extras', '>=0.3.0', true, true)], dependents: [], error: null },
    { name: 'bevy-brp', version: '0.3.0', enabled: false, depends_on: [edge('missing-helper', '>=2.0.0', false, true)], dependents: [], error: 'dependency "missing-helper" is not installed' },
    { name: 'encoding-guardian', version: '2.1.1', enabled: true, depends_on: [], dependents: [], error: null },
  ],
  'corvus.plugin_enable_preview': { plan: ['source-export', 'release-notes'], blockers: [] },
  'corvus.plugin_disable_preview': ['release-notes', 'source-export'],
  'corvus.plugin_dependents': ['release-notes'],
  'corvus.list_containers': [
    { key: 'release-notes::settings', plugin_name: 'release-notes', id: 'settings', kind: 'modal', layout: 'tree_nav', title: 'Release notes settings', width: '860px', height: '560px', submit_label: 'Save', cancel_label: 'Cancel' },
  ],
  'corvus.get_container': null,
  'corvus.jira_get_auth_status': { authenticated: false, user: null, domain: null, authMethod: null },
  'corvus.list_git_providers': [],
  'corvus.list_plugin_contributions': (p) => {
    const all = [
      { plugin_name: 'release-notes', point: 'release-notes::settings:category', item_id: 'general', priority: 10, payload: { label: 'General', icon: 'settings' } },
      { plugin_name: 'release-notes', point: 'release-notes::settings:category', item_id: 'sources', priority: 20, payload: { label: 'Sources', icon: 'git-branch' } },
      { plugin_name: 'release-notes', point: 'release-notes::settings:section', item_id: 'general-main', priority: 10, payload: { category: 'general', label: 'Output', nodes: [
        { type: 'text', name: 'release-notes::file', label: 'Output file', default: 'RELEASE_NOTES.md' },
        { type: 'toggle', name: 'release-notes::group', label: 'Group by type', default: true },
      ] } },
      { plugin_name: 'release-notes', point: 'release-notes::settings:section', item_id: 'sources-main', priority: 10, payload: { category: 'sources', label: 'Where to read from', nodes: [
        { type: 'select', name: 'release-notes::source', label: 'Source', default: 'commits', options: [{ value: 'commits', label: 'Commits' }, { value: 'mrs', label: 'Merge requests' }] },
      ] } },
    ];
    return p.point ? all.filter((c) => c.point === p.point) : all;
  },

  'platform.marketplace_list_installed': { plugins: marketplacePlugins.filter((p) => p.installed), themes: marketplaceThemes.filter((t) => t.installed) },
  'platform.marketplace_fetch_registry': { plugins: marketplacePlugins, themes: marketplaceThemes },
  'platform.marketplace_refresh_registry': { plugins: marketplacePlugins, themes: marketplaceThemes },
  'platform.marketplace_installed_plugin_names': ['source-export', 'release-notes', 'my-local-tools'],
  'platform.marketplace_get_refresh_hours': 24,

  // ── Schedules ─────────────────────────────────────────────────────────────
  'platform.list_schedules': [
    { key: { namespace: 'plugin', name: 'release-notes/refresh_drafts' }, trigger: { kind: 'fixed_rate', interval: d(900) }, enabled: true, fire_on_load: true, only_when_focused: true },
    { key: { namespace: 'plugin', name: 'encoding-guardian/rescan' }, trigger: { kind: 'cron', expr: '0 */6 * * *' }, enabled: false, fire_on_load: false, only_when_focused: false },
    { key: { namespace: 'marketplace', name: 'refresh_registry' }, trigger: { kind: 'fixed_delay', delay: d(86400) }, enabled: true, fire_on_load: true, only_when_focused: false },
  ],

  // ── Process monitor ───────────────────────────────────────────────────────
  'platform.get_processes_config': { warn_mb: 2048, warn_cpu: 90 },
  'platform.process_report': {
    rows: [
      { pid: 4100, parent: null, name: 'arbor', label: null, kind: 'shell', product: null, cpu: 3.2, memory: 410 * MB, uptime_s: 5400 },
      { pid: 4120, parent: 4100, name: 'corvus-be', label: null, kind: 'backend', product: 'corvus', cpu: 1.1, memory: 260 * MB, uptime_s: 5390 },
      { pid: 4133, parent: 4100, name: 'bennu-be', label: null, kind: 'backend', product: 'bennu', cpu: 12.8, memory: 2.4 * GB, uptime_s: 5380 },
      { pid: 4150, parent: 4133, name: 'rust-analyzer', label: null, kind: 'language-server', product: 'bennu', cpu: 96, memory: 1.1 * GB, uptime_s: 900 },
      { pid: 4177, parent: 4133, name: 'java', label: 'Maven test run', kind: 'child', product: 'bennu', cpu: 44, memory: 780 * MB, uptime_s: 35 },
      { pid: 4178, parent: 4177, name: 'conhost', label: null, kind: 'child', product: 'bennu', cpu: 0.1, memory: 6 * MB, uptime_s: 35 },
      { pid: 4181, parent: 4133, name: 'cargo', label: null, kind: 'child', product: 'bennu', cpu: 18, memory: 120 * MB, uptime_s: 20 },
      { pid: 4182, parent: 4181, name: 'conhost', label: null, kind: 'child', product: 'bennu', cpu: 0, memory: 5 * MB, uptime_s: 20 },
      { pid: 4200, parent: 4100, name: 'merula-be', label: null, kind: 'backend', product: 'merula', cpu: 0.4, memory: 150 * MB, uptime_s: 3000 },
    ],
    total_cpu: 157.5, total_memory: 5.1 * GB, cores: 10, machine_memory: 32 * GB, cpu_sampled: true,
  },
  'platform.process_memory': {
    items: [
      { scope: '/Users/dev/dev/commons-lang', label: 'Symbol index', bytes: 640 * MB, count: 48210, exact: false, mapped: false },
      { scope: '/Users/dev/dev/commons-lang', label: 'Source text', bytes: 210 * MB, count: 1320, exact: true, mapped: false },
      { scope: 'process', label: 'JDK class index', bytes: 380 * MB, count: null, exact: false, mapped: true },
    ],
  },
  'platform.notify_theme_changed': null,
  'platform.list_processes_config': null,
};
