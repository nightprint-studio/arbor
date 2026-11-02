/** Mock answers for the "platform" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const NOW = Math.floor(Date.now() / 1000);
const HOME = '/Users/dev';

export const platformFixtures: Record<string, FixtureHandler> = {
  // ── App / window ──────────────────────────────────────────────────────────
  'platform.get_app_info': { version: '0.14.2', os: 'macOS', arch: 'aarch64' },
  list_running_products: ['corvus', 'bennu'],
  list_windows: [
    { label: 'main', title: 'Arbor', product: null, kind: 'launcher', focused: true, visible: true },
    { label: 'corvus-1', title: 'arbor — Corvus', product: 'corvus', kind: 'product', focused: false, visible: true },
    { label: 'bennu-1', title: 'commons-lang — Bennu', product: 'bennu', kind: 'product', focused: false, visible: true },
  ],
  'plugin:window|available_monitors': [
    { name: 'Built-in Retina Display', position: { x: 0, y: 0 }, size: { width: 2880, height: 1800 }, scaleFactor: 2 },
  ],
  'plugin:window|current_monitor': {
    name: 'Built-in Retina Display', position: { x: 0, y: 0 }, size: { width: 2880, height: 1800 }, scaleFactor: 2,
  },
  set_window_title: null,
  set_native_menu: null,
  frontend_ready: null,
  window_ready: null,

  // ── Config ────────────────────────────────────────────────────────────────
  get_launcher_config: {
    products: {
      corvus: { close_to_tray: true },
      bennu: { close_to_tray: false },
      merula: { close_to_tray: false },
      sitta: { close_to_tray: true },
    },
    window_mode: 'windows',
  },
  'platform.get_appearance_config': {
    window_controls_style: 'mac',
    font_scale: 1,
    use_theme_fonts: false,
    activity_bar_position: 'left',
    compact_title_bar: false,
    parked_modals_max: 5,
    compact_file_tree_dirs: true,
    stage_view_mode: 'list',
  },
  'platform.get_animations_config': { enabled: true, speed: 'normal' },

  // ── Theme ─────────────────────────────────────────────────────────────────
  'platform.get_active_theme_id': 'dark',
  'platform.list_custom_themes': [
    {
      id: 'nord-night', name: 'Nord Night', description: 'Cold, low-contrast blues.', built_in: false,
      vars: { '--bg-base': '#2e3440', '--bg-elevated': '#3b4252', '--text-primary': '#eceff4', '--accent': '#88c0d0' },
    },
    {
      id: 'solar-paper', name: 'Solar Paper', description: 'Warm light theme.', built_in: false,
      vars: { '--bg-base': '#fdf6e3', '--bg-elevated': '#eee8d5', '--text-primary': '#073642', '--accent': '#268bd2' },
    },
  ],

  // ── Jobs ──────────────────────────────────────────────────────────────────
  'platform.list_jobs': [
    { id: 'job-1', name: 'Fetch all remotes', plugin_name: 'arbor', command: 'git fetch --all', started_at: NOW - 42, status: { type: 'running' }, category: 'git' },
    { id: 'job-2', name: 'mvn -q test', plugin_name: 'bennu', command: 'mvn -q test', started_at: NOW - 600, status: { type: 'completed', exit_code: 0 }, category: 'build' },
    { id: 'job-3', name: 'Index project', plugin_name: 'bennu', command: 'index', started_at: NOW - 1800, status: { type: 'failed', error: 'JDK 21 not found' }, category: 'index' },
  ],
  'platform.running_job_count': 1,
  'platform.get_job_output': ['Fetching origin', 'Fetching upstream', 'remote: Counting objects: 128, done.'],

  // ── Recents (launcher + welcome) ──────────────────────────────────────────
  list_recent_projects: [
    { product: 'corvus', path: `${HOME}/dev/arbor`, name: 'arbor', opened_at: NOW - 300, pinned: true },
    { product: 'bennu', path: `${HOME}/dev/commons-lang`, name: 'commons-lang', opened_at: NOW - 3600 },
    { product: 'corvus', path: `${HOME}/dev/geode`, name: 'geode', opened_at: NOW - 86400 },
    { product: 'merula', path: `${HOME}/music/nobody-digs-anymore`, name: 'nobody-digs-anymore', opened_at: NOW - 2 * 86400 },
    { product: 'garrulus', path: `${HOME}/notes/work`, name: 'work', opened_at: NOW - 5 * 86400 },
  ],
  'platform.get_recent_repos': [`${HOME}/dev/arbor`, `${HOME}/dev/geode`, `${HOME}/dev/commons-lang`],
  record_recent_project: null,
  forget_recent_project: null,
  pin_recent_project: null,

  // ── Profiles ──────────────────────────────────────────────────────────────
  list_profiles: ['default', 'work', 'streaming'],
  get_active_profile: 'default',

  // ── Filesystem (read-only bits that dialogs poke at) ──────────────────────
  'platform.fs_read_text_file': (p) =>
    String(p.path ?? '').endsWith('.md')
      ? '# Arbor\n\nA desktop developer workspace.\n\n## Build\n\n```sh\nyarn tauri dev\n```\n\n- keyboard first\n- IntelliJ-like layout\n'
      : 'tracks(\n  s("bd*4").bank("RolandTR909"),\n  s("~ hh*2").gain(0.6),\n)\n',
  'platform.fs_read_dir': [
    { name: 'main.rs', path: `${HOME}/dev/arbor/src/main.rs`, is_dir: false, size: 4821, modified: NOW - 3600, hidden: false },
    { name: 'lib', path: `${HOME}/dev/arbor/src/lib`, is_dir: true, size: 0, modified: NOW - 7200, hidden: false },
  ],
  'platform.fs_dir_size': { bytes: 18_400_000, files: 412, dirs: 37 },

  // ── MCP ───────────────────────────────────────────────────────────────────
  get_mcp_config: {
    enabled: true,
    port: 8787,
    token: 'mock-token-0000',
    products: { bennu: true, corvus: true, tyto: false },
    scope: { mode: 'open_projects' },
    policy: { read: 'allow', write: 'ask', destructive: 'deny' },
    projects: [
      {
        root: `${HOME}/dev/arbor`, name: 'arbor', products: { bennu: true },
        policy: { read: null, write: 'allow', destructive: null }, tools: { bennu_write_file: 'ask' },
      },
    ],
    consent_timeout_secs: 60,
    max_result_bytes: 200000,
  },
  get_mcp_status: { running: true, port: 8787, token: 'mock-token-0000', url: 'http://127.0.0.1:8787/mcp', detail: null },
  get_mcp_audit: {
    run: 1,
    entries: [
      { id: 3, run: 1, at: NOW * 1000 - 5000, tool: 'bennu_write_file', program: 'bennu', safety: 'write', outcome: 'asked_allowed', arguments: '{"path":"src/Main.java"}', duration_ms: 42, detail: null, progress: [] },
      { id: 2, run: 1, at: NOW * 1000 - 60000, tool: 'bennu_find_symbol', program: 'bennu', safety: 'read', outcome: 'allowed', arguments: '{"name":"StringUtils"}', duration_ms: 11, detail: null, progress: [] },
      { id: 1, run: 1, at: NOW * 1000 - 120000, tool: 'tyto_screenshot', program: 'tyto', safety: 'read', outcome: 'denied', arguments: '{}', duration_ms: null, detail: 'product not exposed', progress: [] },
    ],
  },
  get_mcp_clients: {
    clients: [{ name: 'claude-code', version: '2.1.0', protocol: '2025-06-18', first_seen_ms: NOW * 1000 - 3600000, last_seen_ms: NOW * 1000 - 5000, handshakes: 2 }],
    open_streams: 1, requests: 37, last_request_ms: NOW * 1000 - 5000, running: true,
  },
  mcp_grant_list: [],
  get_mcp_tools: [
    {
      program: 'bennu', exposed: true, granted: false, detail: null,
      tools: [
        { name: 'bennu_find_symbol', method: 'find_symbol', title: 'Find symbol', description: 'Locate a declared symbol by name.', safety: 'read', idempotent: true },
        { name: 'bennu_write_file', method: 'write_file', title: 'Write file', description: 'Write a file preserving its encoding.', safety: 'write', idempotent: false },
        { name: 'bennu_test_run', method: 'test_run', title: 'Run tests', description: 'Run the project tests and wait.', safety: 'destructive', idempotent: false },
      ],
    },
    { program: 'tyto', exposed: false, granted: false, detail: null, tools: [
      { name: 'tyto_screenshot', method: 'screenshot', title: 'Screenshot', description: 'Capture a monitor or window.', safety: 'read', idempotent: true },
    ] },
  ],
};
