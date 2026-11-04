/** Mock answers for the "corvus" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const day = 86_400;
const now = () => Math.floor(Date.now() / 1000);
const iso = (secsAgo: number) => new Date(Date.now() - secsAgo * 1000).toISOString();

export const TAB_ID = 'tab-arbor';
export const REPO_PATH = '/Users/christian/sviluppo/mio/workspace/rust/apps/arbor';

const alice = { name: 'Alice Moretti', email: 'alice.moretti@nightprint.dev' };
const marco = { name: 'Marco Bellini', email: 'marco.bellini@nightprint.dev' };

export const branches = [
  { name: 'main', full_ref: 'refs/heads/main', is_head: false, upstream: 'origin/main', ahead: 0, behind: 0, head_oid: 'a41c9e07d2b3', head_summary: 'Merge pull request #212 from feature/lazy-tree' },
  { name: 'feature/bennu', full_ref: 'refs/heads/feature/bennu', is_head: true, upstream: 'origin/feature/bennu', ahead: 3, behind: 1, head_oid: 'f839f73e11aa', head_summary: 'gradle support & attachment on Imber chat' },
  { name: 'develop', full_ref: 'refs/heads/develop', is_head: false, upstream: 'origin/develop', ahead: 0, behind: 4, head_oid: '9c02ab5f77d0', head_summary: 'fix performance workspace swap corvus' },
  { name: 'fix/context-menu-flicker', full_ref: 'refs/heads/fix/context-menu-flicker', is_head: false, ahead: 2, behind: 0, head_oid: '87f82a14c3e9', head_summary: 'fix context menu problem on corvus' },
];

export const remoteBranches = [
  { name: 'origin/main', full_ref: 'refs/remotes/origin/main', is_head: false, ahead: 0, behind: 0, head_oid: 'a41c9e07d2b3', head_summary: 'Merge pull request #212 from feature/lazy-tree' },
  { name: 'origin/develop', full_ref: 'refs/remotes/origin/develop', is_head: false, ahead: 0, behind: 0, head_oid: '9c02ab5f77d0', head_summary: 'fix performance workspace swap corvus' },
  { name: 'origin/feature/bennu', full_ref: 'refs/remotes/origin/feature/bennu', is_head: false, ahead: 0, behind: 0, head_oid: '21715bc2aa90', head_summary: 'lazy project tree bennu & some java validation fix' },
  { name: 'origin/release/0.9', full_ref: 'refs/remotes/origin/release/0.9', is_head: false, ahead: 0, behind: 0, head_oid: '5be31d0c8a12', head_summary: 'Release 0.9.0' },
];

export const commitNode = {
  oid: 'f839f73e11aa2c4b9d0e5f6a7b8c9d0e1f2a3b4c',
  short_oid: 'f839f73',
  summary: 'gradle support & attachment on Imber chat & optional context',
  body: 'Adds Gradle build-model detection and lets the Imber chat attach files.',
  author: alice,
  committer: alice,
  timestamp: now() - 3 * 3600,
  row: 0,
  lane: 0,
  color_index: 1,
  refs: [{ name: 'feature/bennu', ref_type: 'local_branch', is_current: true }],
  is_merge: false,
  is_head: true,
  active_lanes: [0],
};

export const worktree = {
  path: '/Users/christian/sviluppo/mio/workspace/rust/apps/arbor-wt-hotfix',
  branch: 'fix/context-menu-flicker',
  head_sha: '87f82a14c3e9b6d0a1f4e2c8d5b7a9f0c3e6d1b2',
  head_short: '87f82a1',
  is_main: false,
  is_locked: false,
  is_current: false,
  project_type: 'rust',
  ahead: 2,
  behind: 0,
  changes_count: 5,
};

export const submodule = {
  name: 'vendor/libgit2-sys',
  path: 'vendor/libgit2-sys',
  abs_path: `${REPO_PATH}/vendor/libgit2-sys`,
  url: 'https://github.com/rust-lang/git2-rs.git',
  head_hash: 'c7d41ae',
  branch: 'main',
  ahead: 0,
  behind: 2,
  is_dirty: false,
  is_initialized: true,
};

export const mr = {
  number: 214,
  title: 'Lazy project tree for Bennu: load directories on expand',
  description:
    'Loads children of a directory only when it is expanded.\n\n## Changes\n- `bennu_project_tree` returns one level at a time\n- Tree keeps expanded state across refresh\n\n## Testing\nOpened a 40k-file monorepo: first paint went from 3.1s to 180ms.',
  state: 'open',
  isDraft: false,
  author: { login: 'amoretti', displayName: 'Alice Moretti', avatarUrl: null },
  sourceBranch: 'feature/lazy-tree',
  targetBranch: 'main',
  webUrl: 'https://github.com/nightprint-studio/arbor/pull/214',
  createdAt: iso(2 * day),
  updatedAt: iso(3600 * 4),
  labels: [{ name: 'performance', color: '0e8a16' }, { name: 'bennu', color: '1d76db' }],
  assignees: [{ login: 'mbellini', displayName: 'Marco Bellini', avatarUrl: null }],
  reviewers: [{ login: 'mbellini', displayName: 'Marco Bellini', avatarUrl: null }],
  checksStatus: 'success',
  mergeable: true,
  provider: 'github',
  commentsCount: 3,
  squash: false,
  deleteBranch: true,
  headSha: '3f9a1c2d4e5b6a7980c1d2e3f4a5b6c7d8e9f001',
  baseSha: 'a41c9e07d2b3c4d5e6f708192a3b4c5d6e7f8091',
  autoMergeEnabled: false,
};

export const ciRun = {
  id: '9981234567',
  name: 'CI / build-and-test',
  status: 'failed',
  branch: 'feature/lazy-tree',
  commit_sha: '3f9a1c2d4e5b6a7980c1d2e3f4a5b6c7d8e9f001',
  web_url: 'https://github.com/nightprint-studio/arbor/actions/runs/9981234567',
  created_at: iso(3600 * 3),
  provider: 'github',
  duration_secs: 412,
};

export const ciProvider = {
  provider: 'github',
  remote_url: 'https://github.com/nightprint-studio/arbor.git',
  has_token: true,
  owner: 'nightprint-studio',
  repo_name: 'arbor',
  project_path: null,
  gitlab_base_url: null,
};

export const securitySummary = {
  counts: { critical: 2, high: 7, medium: 14, low: 9, info: 3, unknown: 0 },
  median_age_days: { critical: 12, high: 31, medium: 58, low: 90, info: 120, unknown: null },
  risk_score: { value: 68, label: 'High' },
  time_series: null,
  provider_kind: 'github',
  web_url: 'https://github.com/nightprint-studio/arbor/security',
  findings_seen: 35,
  truncated: false,
};

const finding = (id: string, severity: string, title: string, file: string, line: number, ageDays: number, type: string, scanner: string) => ({
  id, severity, state: 'detected', title, description: `${title}. Review the affected code path and apply the suggested fix.`,
  scanner, report_type: type, file_path: file, start_line: line,
  web_url: `https://github.com/nightprint-studio/arbor/security/code-scanning/${id}`,
  created_at: iso(ageDays * day), age_days: ageDays,
  identifiers: [{ kind: 'CWE', value: 'CWE-79', url: 'https://cwe.mitre.org/data/definitions/79.html' }, { kind: 'CVE', value: 'CVE-2025-48711', url: null }],
  provider: 'github', solution: 'Upgrade to the patched version.',
});
export const findings = [
  finding('f1', 'critical', 'Prototype pollution in lodash.merge', 'package.json', 41, 9, 'dependency_scanning', 'Dependabot'),
  finding('f2', 'critical', 'Hard-coded AWS access key', 'src-tauri/src/cloud/s3.rs', 88, 14, 'secret_detection', 'Gitleaks'),
  finding('f3', 'high', 'Path traversal in archive extraction', 'crates/platform/fs/src/unzip.rs', 132, 31, 'sast', 'CodeQL'),
  finding('f4', 'high', 'Regular expression denial of service in semver', 'yarn.lock', 1204, 40, 'dependency_scanning', 'Dependabot'),
  finding('f5', 'medium', 'Use of insecure random number generator', 'src/lib/utils/id.ts', 17, 58, 'sast', 'CodeQL'),
  finding('f6', 'low', 'Missing Content-Security-Policy header', 'src-tauri/tauri.conf.json', 22, 90, 'sast', 'CodeQL'),
];

export const registry = [
  { id: 'r1', path: REPO_PATH, remote_url: 'git@github.com:nightprint-studio/arbor.git', display_name: 'arbor', common_dir: `${REPO_PATH}/.git`, current_branch: 'feature/bennu', is_worktree: false },
  { id: 'r2', path: '/Users/christian/sviluppo/mio/workspace/rust/extensions/arbor-extensions', remote_url: 'git@github.com:nightprint-studio/arbor-extensions.git', display_name: 'arbor-extensions', common_dir: null, current_branch: 'main', is_worktree: false },
  { id: 'r3', path: '/Users/christian/sviluppo/mio/workspace/rust/games/voriz', remote_url: null, display_name: 'voriz', common_dir: null, current_branch: 'develop', is_worktree: false },
  { id: 'r4', path: '/Users/christian/sviluppo/lavoro/gestionale-legacy', remote_url: 'https://gitlab.com/acme/gestionale-legacy.git', display_name: 'gestionale-legacy', common_dir: null, current_branch: 'master', is_worktree: false },
  { id: 'r5', path: '/Users/christian/sviluppo/mio/workspace/rust/geode', remote_url: 'git@github.com:nightprint-studio/geode.git', display_name: 'geode', common_dir: null, current_branch: 'main', is_worktree: false },
];

export const workspaceDef = {
  id: 'ws1', name: 'Arbor platform', color_idx: 2, repo_ids: ['r1', 'r2'], order: 0,
  group_id: null, metadata: null, settings_override: null, git_identity: null,
};

export const remoteAccounts = [
  { provider: 'github', username: 'christian-mantini', display_name: 'Christian Mantini', avatar_url: null },
  { provider: 'gitlab', username: 'cmantini', display_name: 'Christian Mantini', avatar_url: null },
];

const repo = (n: number, ns: string, name: string, lang: string | null, priv: boolean, desc: string | null, provider = 'github') => ({
  id: String(n), name, namespace: ns, full_name: `${ns}/${name}`, description: desc, private: priv,
  default_branch: 'main', language: lang, stars: (n * 37) % 400, updated_at: iso(n * day),
  clone_url_https: `https://${provider}.com/${ns}/${name}.git`, clone_url_ssh: `git@${provider}.com:${ns}/${name}.git`,
  web_url: `https://${provider}.com/${ns}/${name}`, provider, is_fork: n % 5 === 0, is_archived: n === 7, size_kb: 1200 * n,
});
export const remoteRepos = [
  repo(1, 'nightprint-studio', 'arbor', 'Rust', true, 'Git GUI and developer workspace'),
  repo(2, 'nightprint-studio', 'arbor-extensions', 'Lua', false, 'Plugins for Arbor'),
  repo(3, 'nightprint-studio', 'geode', 'Rust', true, 'Audio middleware experiments'),
  repo(4, 'christian-mantini', 'dotfiles', 'Shell', false, null),
  repo(5, 'christian-mantini', 'bevy-fulcrum', 'Rust', false, 'UI toolkit on top of bevy'),
  repo(7, 'christian-mantini', 'old-struts-demo', 'Java', false, 'Archived Struts 1 sample'),
];

const issueStatus = (id: string, name: string, color: string, statusType: string) => ({ id, name, color, statusType });
export const statuses = [
  issueStatus('s1', 'Backlog', '#95a2b3', 'backlog'),
  issueStatus('s2', 'Todo', '#e2e2e2', 'unstarted'),
  issueStatus('s3', 'In Progress', '#f2c94c', 'started'),
  issueStatus('s4', 'In Review', '#5e6ad2', 'started'),
  issueStatus('s5', 'Done', '#4cb782', 'completed'),
];
const me = { id: 'u1', name: 'cmantini', displayName: 'Christian Mantini', avatarUrl: null, email: 'christian@nightprint.dev' };
const issueAt = (n: number, title: string, st: number, prio: number, labels: string[] = []) => ({
  id: `iss-${n}`, identifier: `ARB-${n}`, title,
  description: `Steps to reproduce:\n\n1. Open a repository with more than 5k commits\n2. Scroll the graph quickly\n\n**Expected**: smooth scrolling.\n**Actual**: the lane column flickers.`,
  descriptionFormat: 'markdown', status: statuses[st], priority: prio,
  priorityLabel: ['No priority', 'Urgent', 'High', 'Medium', 'Low'][prio],
  assignee: n % 2 ? me : null, labels: labels.map((l, i) => ({ id: `l${i}`, name: l, color: ['#eb5757', '#4ea7fc', '#bb87fc'][i % 3] })),
  url: `https://linear.app/nightprint/issue/ARB-${n}`, createdAt: iso(n * day), updatedAt: iso(n * 3600),
  dueDate: null, estimate: 3, team: { id: 't1', name: 'Arbor', key: 'ARB' }, project: { id: 'p1', name: 'Bennu', color: '#5e6ad2' },
  cycle: { id: 'c1', name: 'Cycle 14', number: 14 },
  comments: [{ id: 'c1', body: 'Reproduced on macOS 15 with the retina display.', bodyFormat: 'markdown', createdAt: iso(day), user: me }],
  commentCount: 1, attachments: [],
});
export const issues = [
  issueAt(412, 'Graph lane column flickers while scrolling fast', 2, 2, ['bug', 'graph']),
  issueAt(409, 'Add keyboard shortcut for "Create MR from current branch"', 1, 3, ['ux']),
  issueAt(401, 'Stash list loses selection after refresh', 3, 3, ['bug']),
  issueAt(397, 'Bennu: Gradle multi-project import is slow', 4, 2, ['performance', 'bennu']),
  issueAt(388, 'Clone modal: remember last destination folder', 0, 4, ['ux']),
];
export const issueFilterOptions = {
  teams: [{ id: 't1', name: 'Arbor', key: 'ARB' }],
  statuses,
  labels: [{ id: 'l0', name: 'bug', color: '#eb5757' }, { id: 'l1', name: 'ux', color: '#4ea7fc' }, { id: 'l2', name: 'performance', color: '#bb87fc' }],
  issueTypes: [{ id: 'it1', name: 'Bug', color: '#eb5757' }, { id: 'it2', name: 'Feature', color: '#4cb782' }],
  projects: [{ id: 'p1', name: 'Bennu', color: '#5e6ad2' }],
  cycles: [{ id: 'c1', name: 'Cycle 14', number: 14 }],
  milestones: [],
  me,
};

const blameAuthors = [alice, marco];
export const blameLines = Array.from({ length: 60 }, (_, i) => {
  const a = blameAuthors[Math.floor(i / 9) % 2];
  const start = i % 9 === 0;
  const content = [
    'use std::collections::HashMap;', '', 'pub struct ProjectTree {', '    root: PathBuf,', '    expanded: HashSet<PathBuf>,',
    '}', '', 'impl ProjectTree {', '    pub fn new(root: PathBuf) -> Self {',
  ][i % 9];
  return {
    line_no: i + 1, content, commit_oid: `c0ffee${(Math.floor(i / 9)).toString(16).padStart(34, '0')}`,
    short_oid: `c0ffee${Math.floor(i / 9)}`, author_name: a.name, author_email: a.email,
    timestamp: now() - (Math.floor(i / 9) + 1) * 5 * day, summary: ['lazy project tree bennu', 'initial import', 'rename root field', 'fix expanded-state persistence', 'cleanup imports', 'format', 'perf: avoid clone', '', ''][Math.floor(i / 9)] ?? 'misc',
    is_group_start: start,
  };
});

export const repoHealth = [
  { repo_id: 'r1', path: REPO_PATH, missing: false, branch: 'feature/bennu', ahead: 3, behind: 1, has_upstream: true, dirty: true, conflicted: false, detached: false, is_worktree: false, error: null },
  { repo_id: 'r2', path: registry[1].path, missing: false, branch: 'main', ahead: 0, behind: 0, has_upstream: true, dirty: false, conflicted: false, detached: false, is_worktree: false, error: null },
  { repo_id: 'r3', path: registry[2].path, missing: false, branch: null, ahead: 0, behind: 0, has_upstream: false, dirty: false, conflicted: false, detached: true, is_worktree: false, error: null },
  { repo_id: 'r4', path: registry[3].path, missing: true, branch: null, ahead: 0, behind: 0, has_upstream: false, dirty: false, conflicted: false, detached: false, is_worktree: false, error: 'path not found' },
];

const dl = (kind: string, o: number | undefined, n: number | undefined, content: string) => ({ kind, old_lineno: o, new_lineno: n, content });
export const diffFiles = [
  {
    path: 'crates/products/bennu/project/src/tree.rs', status: 'modified', is_binary: false, encoding: 'UTF-8',
    stats: { additions: 4, deletions: 2 },
    hunks: [{
      header: '@@ -10,6 +10,8 @@ impl ProjectTree {', old_start: 10, old_lines: 6, new_start: 10, new_lines: 8,
      lines: [
        dl('context', 10, 10, '    pub fn children(&self, dir: &Path) -> Vec<Node> {'),
        dl('removed', 11, undefined, '        walk(dir)'),
        dl('removed', 12, undefined, '            .collect()'),
        dl('added', undefined, 11, '        let mut out = Vec::new();'),
        dl('added', undefined, 12, '        read_one_level(dir, &mut out);'),
        dl('added', undefined, 13, '        out.sort_by(|a, b| a.name.cmp(&b.name));'),
        dl('added', undefined, 14, '        out'),
        dl('context', 13, 15, '    }'),
      ],
    }],
  },
  { path: 'src/lib/components/bennu/BennuProjectTree.svelte', status: 'modified', is_binary: false, stats: { additions: 37, deletions: 12 }, hunks: [] },
  { path: 'docs/lazy-tree.md', status: 'added', is_binary: false, stats: { additions: 22, deletions: 0 }, hunks: [] },
  { path: 'old/legacy-walker.rs', status: 'deleted', is_binary: false, stats: { additions: 0, deletions: 88 }, hunks: [] },
];

const stepRun = (name: string, status: string, out: string[], code: number | null) => ({ def_id: name, name, status, output: out, started_at: now() - 300, finished_at: status === 'running' ? null : now() - 240, exit_code: code });
export const pipelineRun = {
  id: 'run-7f3a', pipeline_id: 'release-build', plugin: 'release-tools', name: 'Release build 0.9.1',
  status: 'failed', started_at: now() - 420, finished_at: now() - 60,
  stages: [
    { def_id: 'prepare', name: 'Prepare', status: 'success', steps: [stepRun('Checkout tag', 'success', ['HEAD is now at 5be31d0 Release 0.9.0'], 0), stepRun('Install dependencies', 'success', ['yarn install v1.22.22', 'Done in 14.2s.'], 0)] },
    { def_id: 'build', name: 'Build', status: 'failed', steps: [stepRun('Compile backend', 'failed', ['   Compiling arbor v0.9.1', 'error[E0432]: unresolved import `corvus_core::prelude::CorvusState`', 'error: could not compile `arbor` (bin "arbor") due to 1 previous error'], 101), stepRun('Bundle frontend', 'pending', [], null)] },
  ],
};

const planRepo = (name: string, workspace: string) => ({ path: `C:/Work/${workspace}/${name}`, name, workspace });
const planStep = (id: string, spec: Record<string, unknown>, repo: ReturnType<typeof planRepo>, status = 'queued', extra: Record<string, unknown> = {}) =>
  ({ id, ...spec, repo, status, error: null, started_ms: null, finished_ms: null, result: null, ...extra });
const gate = (spec: Record<string, unknown>, state = 'idle') => ({ ...spec, state, since_ms: null });
const gw = planRepo('api-gateway', 'Platform');
const wc = planRepo('web-client', 'Platform');
const ic = planRepo('infra-config', 'Ops');
const bs = planRepo('billing-service', 'Billing');
const mrSpec = (title: string) => ({ kind: 'create_mr', source: 'feature/auth-v2', target: 'main', title, description: '', labels: [], draft: false, auto_merge: false, squash: false, delete_branch: false });

/** Plans for the gallery: one waiting on a merge, one paused on a failed push, one draft, one template. */
export const plans = [
  {
    id: 'plan-release', name: 'Release 2.4', status: 'running', pause_reason: null, template: false, created_ms: 1, updated_ms: 5,
    phases: [
      { id: 'ph1', name: 'Publish', gate: gate({ kind: 'phase_done' }, 'open'), steps: [
        planStep('s1', { kind: 'push', branch: 'feature/auth-v2' }, gw, 'done', { finished_ms: 1800 }),
        planStep('s2', { kind: 'push', branch: 'feature/auth-v2' }, wc, 'done'),
        planStep('s3', { kind: 'push', branch: 'chore/bump-redis' }, ic, 'done'),
      ] },
      { id: 'ph2', name: 'Review', gate: gate({ kind: 'phase_done' }, 'open'), steps: [
        planStep('s4', mrSpec('Add OAuth2 PKCE flow'), gw, 'done', { result: { number: 482, url: 'https://example.test/482' } }),
        planStep('s5', mrSpec('Use PKCE login client'), wc, 'done', { result: { number: 117, url: 'https://example.test/117' } }),
      ] },
      { id: 'ph3', name: 'Roll out', gate: gate({ kind: 'mr_merged', step_id: 's4', poll_secs: 30, timeout_secs: 7200 }, 'waiting'), steps: [
        planStep('s6', { kind: 'merge_mr', from_step: 's5', number: null, squash: false, delete_branch: false }, wc, 'held'),
        planStep('s7', { kind: 'run_pipeline', branch: 'main', workflow_id: null, variables: [] }, ic, 'held'),
        planStep('s8', { kind: 'run_pipeline', branch: 'main', workflow_id: null, variables: [] }, bs, 'held'),
      ] },
    ],
    finally: [planStep('s9', { kind: 'tag', name: 'v2.4.0', message: null, target: 'main' }, gw, 'queued')],
  },
  {
    id: 'plan-hotfix', name: 'Hotfix auth', status: 'paused', pause_reason: '1 step(s) failed in “Publish”', template: false, created_ms: 1, updated_ms: 4,
    phases: [
      { id: 'hp1', name: 'Publish', gate: gate({ kind: 'phase_done' }), steps: [
        planStep('h1', { kind: 'push', branch: 'hotfix/session' }, gw, 'done'),
        planStep('h2', { kind: 'push', branch: 'hotfix/session' }, wc, 'failed', { error: 'rejected: non-fast-forward\nthe remote has 2 commits you do not have' }),
      ] },
      { id: 'hp2', name: 'Review', gate: gate({ kind: 'phase_done' }), steps: [planStep('h3', mrSpec('Fix session fixation'), gw, 'held')] },
    ],
    finally: [],
  },
  {
    id: 'plan-draft', name: 'Weekly deps bump', status: 'draft', pause_reason: null, template: false, created_ms: 1, updated_ms: 3,
    phases: [{ id: 'dp1', name: 'Phase 1', gate: gate({ kind: 'phase_done' }), steps: [
      planStep('d1', { kind: 'pull' }, gw), planStep('d2', { kind: 'pull' }, bs),
    ] }],
    finally: [],
  },
  { id: 'plan-tpl', name: 'Release template', status: 'draft', pause_reason: null, template: true, created_ms: 1, updated_ms: 2, phases: [], finally: [] },
];

export const corvusFixtures: Record<string, FixtureHandler> = {
  'corvus.plan_list': plans,
  'plugin:window|available_monitors': [],
  'plugin:window|current_monitor': { name: 'Built-in Retina Display', position: { x: 0, y: 0 }, size: { width: 2880, height: 1800 }, scaleFactor: 2 },

  'corvus.get_git_identity': ['Christian Mantini', 'christian@nightprint.dev'],
  'corvus.get_issues_config': { sort_field: 'updated_at', sort_dir: 'desc' },
  'corvus.get_mr_capabilities': { autoMergeSupported: true, autoMergeReason: null },
  'corvus.list_local_branches': branches,
  'corvus.list_remote_branches': remoteBranches,
  'corvus.rb_list_accounts': remoteAccounts,
  'corvus.rb_list_repos': remoteRepos,
  'corvus.rb_browse_tree': [
    { name: 'crates', path: 'crates', entry_type: 'dir', size: null },
    { name: 'src', path: 'src', entry_type: 'dir', size: null },
    { name: 'Cargo.toml', path: 'Cargo.toml', entry_type: 'file', size: 2310 },
    { name: 'README.md', path: 'README.md', entry_type: 'file', size: 5120 },
  ],
  'corvus.sync_pull_preview': {
    available: true,
    workspaces: [
      { id: 'ws1', name: 'Arbor platform', status: 'changed', repo_count: 4 },
      { id: 'ws2', name: 'Legacy Struts', status: 'new', repo_count: 2 },
      { id: 'ws3', name: 'Games', status: 'same', repo_count: 3 },
    ],
    settings: [{ key: 'profile', label: 'Profile & theme', differs: true }, { key: 'corvus', label: 'Git settings', differs: false }],
    mods: [{ name: 'git-graph-tweaks', version: '1.2.0', installed: false, enabled: false }],
    plugin_data: [{ name: 'source-export', differs: true }],
    missing_repos: [{ remote_url: 'git@github.com:nightprint-studio/geode.git', display_name: 'geode' }],
  },

  'corvus.list_registry_with_roots': registry,
  'corvus.list_registry_repos': registry,
  'corvus.list_workspaces': { workspaces: [workspaceDef], groups: [], active_workspace_id: 'ws1' },
  'corvus.workspace_health_scan': repoHealth,

  'corvus.get_mr_detail': () => ({
    mr,
    comments: [
      { id: 'c1', author: mr.reviewers[0], body: 'Nice, the first paint is way better. One question on the expanded-state key.', createdAt: iso(3600 * 20), isBot: false },
      { id: 'c2', author: { login: 'github-actions[bot]', displayName: 'github-actions', avatarUrl: null }, body: 'Benchmark: +0.4% binary size.', createdAt: iso(3600 * 18), isBot: true },
    ],
    events: [{ id: 'e1', kind: 'label', actor: mr.author, summary: 'added the **performance** label', createdAt: iso(day) }],
    checks: [
      { name: 'build-and-test', status: 'success', url: null },
      { name: 'clippy', status: 'success', url: null },
      { name: 'frontend-check', status: 'running', url: null },
    ],
  }),
  'corvus.get_mr_files': [
    { filename: 'crates/products/bennu/project/src/tree.rs', status: 'modified', additions: 84, deletions: 21, patch: '@@ -10,6 +10,9 @@ impl ProjectTree {\n     pub fn children(&self, dir: &Path) -> Vec<Node> {\n-        walk(dir)\n+        let mut out = Vec::new();\n+        read_one_level(dir, &mut out);\n+        out\n     }' },
    { filename: 'src/lib/components/bennu/BennuProjectTree.svelte', status: 'modified', additions: 37, deletions: 12, patch: null },
  ],
  'corvus.get_mr_commits': [
    { sha: '3f9a1c2d4e5b', message: 'bennu: load directory children on expand', author: 'Alice Moretti', date: iso(2 * day), webUrl: null },
    { sha: '7b2e9c10aa41', message: 'bennu: keep expanded state across refresh', author: 'Alice Moretti', date: iso(day), webUrl: null },
  ],
  'corvus.get_mr_config': { show_activity: true },
  'corvus.get_repo_config': { issue_tracker: 'linear' },
  'corvus.fetch_mr_ci_runs': [ciRun],
  'corvus.fetch_ci_runs': [ciRun, { ...ciRun, id: '9981234000', name: 'CI / build-and-test', status: 'success', duration_secs: 388, created_at: iso(day) }],
  'corvus.get_ci_provider': ciProvider,
  'corvus.fetch_ci_jobs': [
    { id: 'j1', name: 'lint', stage: 'Jobs', status: 'success', duration_secs: 41, web_url: '', allow_failure: false },
    { id: 'j2', name: 'build (macos-14)', stage: 'Jobs', status: 'success', duration_secs: 301, web_url: '', allow_failure: false },
    { id: 'j3', name: 'test (ubuntu-22.04)', stage: 'Jobs', status: 'failed', duration_secs: 70, web_url: '', allow_failure: false },
    { id: 'j4', name: 'coverage', stage: 'Jobs', status: 'cancelled', duration_secs: null, web_url: '', allow_failure: true },
  ],
  'corvus.list_ci_workflows': [
    { id: 'w1', name: 'CI', path: '.github/workflows/ci.yml' },
    { id: 'w2', name: 'Release', path: '.github/workflows/release.yml' },
  ],

  'corvus.fetch_security_summary': securitySummary,
  'corvus.fetch_security_findings': findings,
  'corvus.supports_security': true,

  'corvus.linear_get_filter_options': issueFilterOptions,
  'corvus.jira_get_filter_options': issueFilterOptions,
  'corvus.linear_search_issues': issues,
  'corvus.jira_search_issues': issues,
  'corvus.linear_get_issue': issues[0],
  'corvus.linear_get_auth_status': { authenticated: true, user: me },
  'corvus.find_commits_for_ticket': [],
  'corvus.branch_name_for_issue': 'feature/arb-412-graph-lane-flicker',

  // BranchCleanupModal reloads in a loop when this resolves null (see report): always answer an array.
  'corvus.list_merged_branches': [branches[3], branches[2]],
  'corvus.list_merged_remote_branches': [remoteBranches[3]],
  'corvus.list_tags': [
    { name: 'v0.9.0', target_oid: '5be31d0c8a12', message: 'Release 0.9.0' },
    { name: 'v0.8.2', target_oid: '1d7e44a09bc3', message: 'Hotfix: graph flicker' },
    { name: 'nightly-2026-10-30', target_oid: 'f839f73e11aa' },
  ],
  'corvus.list_stashes': [
    { index: 0, message: 'WIP on feature/bennu: gradle importer', oid: 'b0b1b2b3c4c5' },
    { index: 1, message: 'On develop: experiment with lane colors', oid: 'd4d5d6d7e8e9' },
  ],
  'corvus.list_pipeline_runs': [pipelineRun],
  'corvus.list_pipeline_defs': [],
  'corvus.fire_plugin_action': null,
  'corvus.probe_mr_feature': { enabled: true, reason: null },
  'corvus.get_git_commit_template': null,
  'corvus.list_mrs': () => [
    mr,
    { ...mr, number: 211, title: 'Fix graph lane flicker on retina displays', sourceBranch: 'fix/lane-flicker', isDraft: true, checksStatus: 'failed', author: mr.reviewers[0], labels: [{ name: 'bug', color: 'd73a4a' }], commentsCount: 0 },
    { ...mr, number: 208, title: 'Gradle importer: support multi-project builds', sourceBranch: 'feature/gradle-multi', checksStatus: 'pending', labels: [], commentsCount: 7 },
  ],
  'corvus.get_branch_diff': diffFiles,
  'corvus.submodule_list_branches': ['main', 'develop', 'release/1.9', 'wip/vendored-openssl'],
  'corvus.get_file_blame_streaming': blameLines,
};
