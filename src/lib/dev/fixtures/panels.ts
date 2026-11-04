import type { FixtureHandler } from './index';
import { shortTypeName } from '$lib/components/bennu/brp/brp-value';
import { bennuFixtures } from './bennu';
import { DEMO_PROJECT } from '$lib/stores/bennu/bennu-mock';

/** Mock backend answers for the "panels" gallery scenes (Imber in Corvus, Bevy Remote, Node, encoding), keyed by `program.method`. */

// ── Imber in Corvus ─────────────────────────────────────────────────────────────────────────────

const IMBER_TOPIC = 'arbor://corvus/imber';

/** The run the mock plays after `imber_send`: a reply with receipts, parked on a commit approval. */
function playImberRun(conversationId: string, runId: string): void {
  const at = Date.now();
  const message = {
    id: 'a-demo-1', role: 'assistant', at, model: 'claude-sonnet-4-5',
    blocks: [
      { type: 'text', text: 'I will check what is staged first.' },
      { type: 'tool', call: { id: 't1', kind: 'read', label: 'git status', detail: '4 files staged on feature/bennu, 1 not staged', ok: true } },
      { type: 'tool', call: { id: 't2', kind: 'read', label: 'git diff --staged', detail: '+312 −48 across 4 files', ok: true } },
      { type: 'tool', call: { id: 't3', kind: 'search', label: 'git log -5', detail: 'Last subjects use lowercase, “&” between topics', ok: true } },
      { type: 'text', text: 'The staged changes add Gradle build-model detection and let the Imber chat attach files. A message in the style of this repository:\n\n> **gradle support & attachments on the Imber chat**\n\nThis is the first write, so I need your go-ahead before I commit.' },
      {
        type: 'approval', promptId: 'prompt-demo-1',
        approval: {
          id: 'ap1', tool: 'git_commit', title: 'Commit 4 staged files', risk: 'write', answer: null, result: null,
          commands: ['git commit -m "gradle support & attachments on the Imber chat"'],
          effect: 'Creates one commit on feature/bennu with the four staged files. Nothing is pushed yet.',
        },
      },
    ],
  };
  const emit = (e: Record<string, unknown>) => window.__mock?.emit(IMBER_TOPIC, { run_id: runId, conversation_id: conversationId, ...e });
  setTimeout(() => emit({ kind: 'message', message }), 60);
}

const imberFixtures: Record<string, FixtureHandler> = {
  'corvus.imber_state': { hasKey: true, model: 'claude-sonnet-4-5', modelName: 'Claude Sonnet 4.5', effort: 'medium', mode: 'manual', sessionLimitEur: 0 },
  assistant_has_key: true,
  'corvus.imber_set_mode': (p) => p.mode,
  'corvus.imber_send': (p) => {
    const conversationId = String(p.args?.conversation_id ?? 'c-demo');
    playImberRun(conversationId, 'run-demo-1');
    return { run_id: 'run-demo-1', conversation_id: conversationId };
  },
  'corvus.imber_reply': true,
  'corvus.imber_cancel': true,
  'corvus.imber_forget': null,
};

// ── Bevy Remote ─────────────────────────────────────────────────────────────────────────────────

const ENDPOINT = 'http://127.0.0.1:15702';
const T = {
  transform: 'bevy_transform::components::transform::Transform',
  name: 'bevy_ecs::name::Name',
  childOf: 'bevy_ecs::hierarchy::ChildOf',
  children: 'bevy_ecs::hierarchy::Children',
  visibility: 'bevy_render::view::visibility::Visibility',
  health: 'dungeon_crawl::combat::Health',
  player: 'dungeon_crawl::player::Player',
  inventory: 'dungeon_crawl::player::Inventory',
  state: 'dungeon_crawl::player::PlayerState',
  goblin: 'dungeon_crawl::enemy::Goblin',
  camera: 'bevy_camera::components::Camera3d',
  light: 'bevy_light::directional_light::DirectionalLight',
  node: 'bevy_ui::ui_node::Node',
  text: 'bevy_ui::widget::text::Text',
};
const R = {
  settings: 'dungeon_crawl::settings::GameSettings',
  score: 'dungeon_crawl::score::Score',
  wave: 'dungeon_crawl::wave::WaveTimer',
  clear: 'bevy_camera::clear_color::ClearColor',
  time: 'bevy_time::time::Time<bevy_time::real::Real>',
  ambient: 'bevy_light::ambient_light::GlobalAmbientLight',
  appState: 'bevy_state::state::resources::State<dungeon_crawl::app::AppState>',
  appNext: 'bevy_state::state::resources::NextState<dungeon_crawl::app::AppState>',
  paused: 'bevy_state::state::resources::State<dungeon_crawl::app::Paused>',
};
const APP_STATE = 'dungeon_crawl::app::AppState';

interface World { id: number; name: string; parent: number | null; components: Record<string, unknown> }
const tf = (x: number, y: number, z: number, s = 1) => ({ translation: [x, y, z], rotation: [0, 0, 0, 1], scale: [s, s, s] });
const base = (id: number, name: string, parent: number | null, x: number, y: number, z: number, extra: Record<string, unknown> = {}): World => ({
  id, name, parent,
  components: { [T.name]: name, [T.transform]: tf(x, y, z), [T.visibility]: 'Inherited', ...extra },
});

const WORLD: World[] = [
  base(101, 'Main Camera', null, 0, 9, 12, { [T.camera]: {} }),
  base(102, 'Sun', null, 0, 20, 0, { [T.light]: { color: { Srgba: { red: 1, green: 0.96, blue: 0.88, alpha: 1 } }, illuminance: 10000, shadows_enabled: true } }),
  base(103, 'Dungeon', null, 0, 0, 0),
  base(110, 'Player', 103, 2.5, 0.9, -4, {
    [T.player]: { speed: 6.5, jump_height: 1.2, facing: 'North' },
    [T.health]: { current: 72, max: 100 },
    [T.inventory]: { gold: 340, slots: [{ item: 'Iron Sword', count: 1 }, { item: 'Health Potion', count: 3 }, { item: 'Torch', count: 5 }] },
    [T.state]: 'Running',
  }),
  base(111, 'Player Mesh', 110, 0, 0, 0),
  base(112, 'Weapon Socket', 110, 0.4, 1.1, 0.2),
  base(120, 'Goblin Camp', 103, -8, 0, 6),
  base(121, 'Goblin', 120, -7, 0.9, 5, {
    [T.transform]: { translation: [-7, 0.9, 5], rotation: [0, 0.2588, 0, 0.9659], scale: [1, 1, 1.5] },
    [T.goblin]: { aggro_range: 9, patrol: true }, [T.health]: { current: 18, max: 30 },
  }),
  base(122, 'Goblin', 120, -9.5, 0.9, 7.5, { [T.goblin]: { aggro_range: 9, patrol: false }, [T.health]: { current: 30, max: 30 } }),
  base(123, 'Goblin Chief', 120, -8, 0.9, 8.5, { [T.goblin]: { aggro_range: 14, patrol: false }, [T.health]: { current: 120, max: 120 } }),
  base(130, 'Ground', 103, 0, 0, 0),
  base(105, 'Torch', 103, 3, 2.2, -2, {
    'bevy_light::point_light::PointLight': { color: { LinearRgba: { red: 1, green: 0.6, blue: 0.2, alpha: 1 } }, intensity: 800000, range: 18, shadows_enabled: false },
    'dungeon_crawl::fx::Flicker': { speed: 6.5, amount: 0.35, started: 1730678400, period: { secs: 1, nanos: 500000000 } },
  }),
  base(104, 'UI Root', null, 0, 0, 0, { [T.node]: { width: { Percent: 100 }, height: { Percent: 100 } } }),
  base(140, 'Health Bar', 104, 0, 0, 0, { [T.node]: { width: { Px: 240 }, height: { Px: 18 } } }),
  base(141, 'Score Text', 104, 0, 0, 0, { [T.text]: 'Score 12450' }),
];

/** What a marker component says the entity is — the backend finds this with a `has` query. */
const kindOfWorld = (w: World): string | null =>
  T.camera in w.components ? 'camera'
    : T.light in w.components || 'bevy_light::point_light::PointLight' in w.components ? 'light'
      : T.node in w.components ? 'ui'
        : T.text in w.components ? 'text'
          : /Mesh|Ground/.test(w.name) ? 'mesh'
            : null;

const rowOf = (w: World) => ({ id: w.id, name: w.name, parent: w.parent, child_count: WORLD.filter((c) => c.parent === w.id).length, kind: kindOfWorld(w) });

const CAN = {
  watch: true, spawn: true, despawn: true, insert: true, remove: true, mutate: true, reparent: true,
  resources: true, mutate_resource: true, insert_resource: true, remove_resource: true,
};
const BRP_STATUS = {
  connected: true, state: 'connected', endpoint: ENDPOINT, family: 'world', discovered: true,
  method_count: 38, type_count: 412, component_types: 287, resource_types: 61, can: CAN, generation: 1, non_loopback: false,
};

const RESOURCES: Record<string, unknown> = {
  [R.score]: { value: 12450, combo: 3, best_combo: 11 },
  [R.settings]: { difficulty: 'Normal', master_volume: 0.8, show_fps: true, vsync: true },
  [R.wave]: { wave: 4, remaining: 14.2, enemies_left: 6 },
  [R.clear]: { Srgba: { red: 0.04, green: 0.05, blue: 0.08, alpha: 1 } },
  [R.time]: { delta_secs: 0.0069, elapsed_secs: 1284.6 },
  [R.ambient]: { brightness: 220, affects_lightmapped_meshes: true },
  [R.appState]: 'Playing',
  [R.appNext]: 'Unchanged',
  [R.paused]: { 0: 'Running' },
};
const shortOf = shortTypeName;
const typeInfo = (p: string) => ({ type_path: p, short_path: shortOf(p), crate_name: p.split('::')[0], reflect_types: ['Component', 'Serialize', 'Deserialize'], kind: 'struct' });
const COMPONENT_TYPES = [...new Set([...WORLD.flatMap((w) => Object.keys(w.components)), T.childOf, T.children])].map(typeInfo);
const RESOURCE_TYPES = Object.keys(RESOURCES).map(typeInfo);

/** 240 frame times around 7 ms with a few hitches, deterministic so the screenshot is stable. */
const FRAMES = Array.from({ length: 240 }, (_, i) => {
  const base = 6.9 + Math.sin(i / 7) * 0.5 + Math.sin(i / 2.3) * 0.2;
  return +(i % 61 === 40 ? base + 9.5 : i % 37 === 11 ? base + 4.2 : base).toFixed(2);
});

// The base64 PNG below is a 480x270 stand-in for the game window.
const SHOT_PNG = 'iVBORw0KGgoAAAANSUhEUgAAAeAAAAEOCAIAAADe+FMwAAAFDUlEQVR42u3aoUpEQRSA4X0ci0EQgwoWuSAi6CIa3KDFIggWEbb4AAaDwWAwGEQMBrPZvA9h9hFMigiGnXtnOHvnW76snD1n/rSDueYKgIAGvgIAgQZAoAEEGgCBBhBoAAQagKkDPfAp8nGCgEALNNCvQF9MTisn0IBACzSAQAs0INACLdCAQAs0INACLdCAn9n5mR0g0AIt0EC8QM9vXgMQkEADCDQAAg0g0AAINIBAAyDQAAg0gEADINAAAg2AQAMg0AACDYBAAwg0AAINUGegF3ZuAAhIoAEEGgCBBhBoAAQaQKABEGgABBpAoAEQaACBBkCgARBoAIEGQKABBBoAgQaoM9BLe7cABCTQAAINgEADCDQAAg0g0AAINAACDSDQAAg0gEADINAACDSAQAMg0AACDYBAA9QZ6JWDOwACEmgAgQZAoAEEGgCBBhBoAAQaAIEGEGgABBpAoAEQaAAEGkCgARBoAIEGQKAB6gz02tE9AAEJNIBAAyDQAAINgEADCDQAAg2AQAMINAACDSDQAAg0AAININAACDSAQAMg0AB1Bnr9+AGAgAQaQKABEGgAgQZAoAEEGgCBBkCgAQQaAIEGEGgABBoAgQYQaPpotHtYjG+bOu9ZoBFoEGgctEDjngUagYY+BHrj5BESlDxo3zZ13rNAI9Ag0DhogcY9CzQCDQKNgxZo3LNA46BBoEGgcc8CjYMWaNyzQCPQINA4aIHGPQu0BZsX+xVoHLRAu2f3LNAOun8H/fn+lEyg3bNA46AFGvcs0A5aoAXaPQu0QFuwQOOeBRoHLdDuWaAF2kELNO65nkBvnT3TiZILnq152wfadbnnOucVaAct0LhngXbQAi3Q7lmgBdqCBRr3LNA4aIF2zwIt0A5aoHHPAm3B5m0/72S7+dEm0L//jv2aV6At2LwCbb/mFWgLFmiBds/mFWgHLdD2a16BtmDz/g10V+zXvAJtweYVaPs1r0BbsEALtHs2r0A7aIG2X/MKtAWbV6Dds3kF2oIF2n7NK9AWbF6Btl/zCrQFC7RAu2fzfgd6eP6SScmB801h3rR5cwTafs1b27wCbV6Btl/zCrQFC7RAu2fzCrQFC7T9mlegLdi8Au2ezSvQFizQ9mtegbZg8wq0/ZpXoC1YoAXaPZtXoB20QEfd78fbakvuWaAt2LwCLdDer0BbsEALtEB7vwJtXoEWaPcs0BYs0AIt0N6vQFuwQAu0exZoCzavQM9uoKf9L96vQFuwQAu0QHu/Am1egRZogRZowRJogRZo7zcl0Pvj10xKDpxvCvOmzZsj0H3db/tAp00UJ9De738E2rwCLdACLdACLdACLdDer0BbsEALtEALtGCZV6AF2vsVaAsWaIEWaIEWLPMKtEALtEBbsEALtEB7vwIt0AIt0AIt0IJlXoEWaO+3m0APLxsAAhJoAIEGQKAB+hDo5dEiAAEJNIBAAyDQAAINgEADCDQAAg2AQAMINAACDSDQAAg0AAININAACDSAQAMg0AACDYBAAyDQAAINgEADCDQAAg2AQAMINAACDSDQAAg0gEADINAACDSAQAMg0AACDYBAAyDQAAINgEADCDQAAg0g0L4CAIEGQKABBBoAgQYQaAAEGgCBBhBoAAQaQKABEGgABBpAoAEQaACBBkCgAQQaAIEGQKABBBoAgQYQaAAEGgCBBhBoAAQaQKABEGgAgQZAoAEQaACBBkCgASrxBQBI3I67mPxHAAAAAElFTkSuQmCC';

const brpFixtures: Record<string, FixtureHandler> = {
  'bennu.bennu_brp_status': BRP_STATUS,
  'bennu.bennu_brp_connect': BRP_STATUS,
  'bennu.bennu_brp_disconnect': { ...BRP_STATUS, connected: false, state: 'disconnected' },
  'bennu.bennu_brp_refresh': { revision: 'r1', total: WORLD.length, changed: false, generation: 1 },
  'bennu.bennu_brp_entities': (p) => {
    const a = p.args ?? {};
    let rows = WORLD;
    if (a.search || a.with || a.without) {
      const q = String(a.search ?? '').toLowerCase();
      rows = WORLD.filter((w) => (!q || w.name.toLowerCase().includes(q))
        && (a.with ?? []).every((t: string) => t in w.components) && !(a.without ?? []).some((t: string) => t in w.components));
    } else {
      rows = WORLD.filter((w) => (a.parent === undefined ? w.parent === null : w.parent === a.parent));
    }
    // `with_parents`: the hits' ancestors come along, marked as context.
    const context: number[] = [];
    if (a.with_parents && (a.search || a.with)) {
      const have = new Set(rows.map((w) => w.id));
      for (const w of [...rows]) {
        for (let up = WORLD.find((x) => x.id === w.parent); up; up = WORLD.find((x) => x.id === up!.parent)) {
          if (!have.has(up.id)) { have.add(up.id); context.push(up.id); rows = [...rows, up]; }
        }
      }
    }
    return { revision: 'r1', total: rows.length - context.length, rows: rows.map(rowOf), context, generation: 1 };
  },
  'bennu.bennu_brp_ancestors': (p) => {
    const out: number[] = [];
    for (let w = WORLD.find((x) => x.id === p.args.id); w?.parent != null; w = WORLD.find((x) => x.id === w!.parent)) out.unshift(w.parent);
    return out;
  },
  'bennu.bennu_brp_entity': (p) => {
    const w = WORLD.find((x) => x.id === p.args.id) ?? WORLD[0];
    const kids = WORLD.filter((c) => c.parent === w.id).map((c) => c.id);
    const components = { ...w.components, ...(w.parent !== null ? { [T.childOf]: w.parent } : {}), ...(kids.length ? { [T.children]: kids } : {}) };
    return { id: w.id, name: w.name, parent: w.parent, components, errors: {}, generation: 1 };
  },
  'bennu.bennu_brp_watch_entity': null,
  'bennu.bennu_brp_unwatch': null,
  'bennu.bennu_brp_resources': Object.keys(RESOURCES).map((p) => ({ type_path: p, short_path: shortOf(p) })),
  'bennu.bennu_brp_resource': (p) => ({ type_path: p.args.type_path, value: RESOURCES[p.args.type_path] }),
  'bennu.bennu_brp_types': (p) => {
    const a = p.args ?? {};
    const q = String(a.query ?? '').toLowerCase();
    const items = (a.kind === 'resource' ? RESOURCE_TYPES : COMPONENT_TYPES).filter((t) => !q || t.type_path.toLowerCase().includes(q));
    return { total: items.length, items };
  },
  'bennu.bennu_brp_type_shape': (p) => {
    if (p.args.type_path === APP_STATE) {
      const variants = ['Loading', 'MainMenu', 'Playing', 'GameOver'].map((name) => ({ name, kind: 'unit', sample: name }));
      return { shape: { kind: 'enum', type_path: APP_STATE, variants }, skeleton: 'Loading', info: null };
    }
    return null;
  },
  'bennu.bennu_brp_inspect_hints': {
    'dungeon_crawl::combat::Health': {
      '': { description: 'Hit points of anything that can be hurt.' },
      current: { range: [0, 100], step: 1, description: 'Current hit points. At 0 the entity is despawned.' },
      max: { range: [1, 200], step: 1, description: 'The most current can reach.' },
    },
    'dungeon_crawl::player::Player': {
      '': { description: 'The character the player controls.' },
      speed: { range: [0, 12], step: 0.1, unit: 'm/s', label: 'move speed', description: 'Walking speed on flat ground.' },
      jump_height: { range: [0, 3], description: 'How high a jump goes, in metres.' },
    },
    'dungeon_crawl::fx::Flicker': { started: { date: true }, amount: { range: [0, 1], step: 0.01 } },
    'bevy_light::point_light::PointLight': { intensity: { range: [0, 2000000], step: 1000 } },
  },
  'bennu.bennu_brp_saved_get': { endpoints: [{ name: 'Local game', endpoint: ENDPOINT }], queries: [{ name: 'Goblins', search: 'goblin has:Health' }] },
  'bennu.bennu_brp_methods': ['rpc.discover', 'world.query', 'world.get_components', 'world.list_components', 'world.spawn_entity'],
  'bennu.bennu_brp_arbor_info': {
    version: '0.3.1', bevy: '0.17.2', methods: ['arbor.info', 'arbor.screenshot', 'arbor.stats', 'arbor.key', 'arbor.mouse'],
    keys: ['KeyW', 'KeyA', 'KeyS', 'KeyD', 'Space', 'Enter', 'Escape', 'ArrowUp', 'ArrowDown', 'F5'],
    gamepad_buttons: ['South', 'East', 'West', 'North', 'LeftTrigger', 'RightTrigger', 'Start', 'Select'],
    gamepad_axes: ['LeftStickX', 'LeftStickY', 'RightStickX', 'RightStickY'],
    actions: [
      { name: 'toggle_pause', description: 'Open or close the pause menu' },
      { name: 'spawn_goblin', description: 'Spawn a goblin next to the player' },
      { name: 'reset_level', description: 'Reload the current level' },
    ],
    presence: { enabled: true, state: 'connected', label: 'Arbor' },
  },
  'bennu.bennu_brp_screenshot': { mime_type: 'image/png', data: SHOT_PNG, width: 1280, height: 720 },
  'bennu.bennu_brp_stats': {
    uptime_s: 1284,
    frame: { frames: 240, avg_ms: 7.1, min_ms: 6.2, max_ms: 16.4, p50_ms: 6.9, p95_ms: 8.8, p99_ms: 15.9, fps: 141 },
    frame_ms_recent: FRAMES,
    diagnostics: [
      { path: 'entity_count', value: 1842, average: 1840, smoothed: 1841, suffix: '' },
      { path: 'process/cpu_usage', value: 14.2, average: 13.6, smoothed: 13.9, suffix: '%' },
      { path: 'process/mem_usage', value: 612.4, average: 611.0, smoothed: 611.8, suffix: ' MiB' },
      { path: 'render/draw_calls', value: 228, average: 226, smoothed: 227, suffix: '' },
    ],
    window: { title: 'Dungeon Crawl', width: 1280, height: 720, physical_width: 1280, physical_height: 720, scale_factor: 1, focused: true },
  },
  'bennu.bennu_brp_actions': { actions: [] },
  'bennu.bennu_brp_key': null,
  'bennu.bennu_brp_mouse': null,
  'bennu.bennu_brp_presence': { enabled: true, state: 'connected', label: 'Arbor' },
};

// ── Node project (package.json) ─────────────────────────────────────────────────────────────────

export const NODE_ROOT = '/Users/dev/projects/storefront-web';

const script = (name: string, command: string, line: number) => ({ name, command, line });
const NODE_PROJECT = {
  package_manager: 'pnpm', lockfile: 'pnpm-lock.yaml', installed: true,
  packages: [
    {
      id: 'storefront-web', dir: '', manifest: `${NODE_ROOT}/package.json`, version: '2.4.0',
      scripts: [
        script('dev', 'vite dev --port 5173', 6), script('build', 'vite build', 7), script('preview', 'vite preview', 8),
        script('check', 'svelte-check --tsconfig ./tsconfig.json', 9), script('test', 'vitest run', 10),
        script('lint', 'eslint . && prettier --check .', 11), script('format', 'prettier --write .', 12),
      ],
    },
    {
      id: '@storefront/ui', dir: 'packages/ui', manifest: `${NODE_ROOT}/packages/ui/package.json`, version: '1.9.2',
      scripts: [script('build', 'svelte-package -i src -o dist', 5), script('test', 'vitest run --coverage', 6), script('storybook', 'storybook dev -p 6006', 7)],
    },
    {
      id: '@storefront/api-client', dir: 'packages/api-client', manifest: `${NODE_ROOT}/packages/api-client/package.json`, version: '0.14.0',
      scripts: [script('generate', 'openapi-typescript ./openapi.yaml -o src/schema.d.ts', 4), script('build', 'tsup src/index.ts --dts', 5), script('test', 'vitest run', 6)],
    },
  ],
};

const nodeFixtures: Record<string, FixtureHandler> = {
  'bennu.bennu_node_packages': NODE_PROJECT,
  'bennu.bennu_open_project': (p) => (p.args.root === NODE_ROOT
    ? { ...DEMO_PROJECT, root: NODE_ROOT, name: 'storefront-web', modules: [], kind: 'plain', jdk: null, source_encoding: 'UTF-8' }
    : null),
  'bennu.bennu_project_tree': (p) => ({
    name: 'storefront-web', path: p.args.root, is_dir: true, children: [
      { name: 'package.json', path: `${NODE_ROOT}/package.json`, is_dir: false, children: [] },
      { name: 'src', path: `${NODE_ROOT}/src`, is_dir: true, children: [] },
    ],
  }),
  // The Maven answer for every other project; the Node one only for the Node project.
  'bennu.bennu_project_modules': (p) => {
    if (p.args?.root === NODE_ROOT) return [{ kind: 'Node', dir: '.' }];
    const dflt = bennuFixtures['bennu.bennu_project_modules'];
    return typeof dflt === 'function' ? dflt(p) : dflt;
  },
};

// ── Encoding conversion ─────────────────────────────────────────────────────────────────────────

const DEMO_BASE = 'demo://GestionaleAtti';
const MODEL = 'core/src/main/java/it/comune/gestionale/core';
const file = (rel: string, status: string, o: Record<string, unknown> = {}) => ({
  file: `${DEMO_BASE}/${rel}`, rel, status, from: 'windows-1252', detected: 'windows-1252', size_before: 4200, size_after: 4310,
  bom_before: false, bom_after: false, unrepresentable_total: 0, samples: [], declaration: null, declaration_rewritten: false, message: null, ...o,
});
const CONVERT_PLAN = {
  files: [
    file(`${MODEL}/model/Bando.java`, 'convert', { size_before: 5120, size_after: 5188 }),
    file(`${MODEL}/model/Allegato.java`, 'convert', { size_before: 2210, size_after: 2236 }),
    file(`${MODEL}/service/BandoServiceImpl.java`, 'convert', { size_before: 9840, size_after: 9971 }),
    file('web/src/main/resources/messages_it.properties', 'convert', { size_before: 3380, size_after: 3501 }),
    file('web/src/main/webapp/WEB-INF/jsp/bando.jsp', 'convert', { size_before: 6120, size_after: 6177, declaration: 'pageEncoding="ISO-8859-1"', declaration_rewritten: true }),
    file('web/src/main/resources/struts.xml', 'convert', { size_before: 2890, size_after: 2899, declaration: 'encoding="ISO-8859-1"', declaration_rewritten: true }),
    file('batch/src/main/java/it/comune/gestionale/batch/ChiusuraScaduti.java', 'convert', { size_before: 3100, size_after: 3140, unrepresentable_total: 0 }),
    file('web/src/main/webapp/legacy/report.jsp', 'invalid', { from: 'UTF-8', detected: 'windows-1252', message: 'Not valid UTF-8' }),
    file('web/src/main/webapp/img/logo.png', 'binary', { message: 'Binary file, left alone' }),
  ],
  truncated: false, total_files: 46, to_convert: 7, unchanged: 37, skipped: 1, blocked: 0, unrepresentable_total: 0,
  project: { declared: 'Cp1252', can_declare: true, whole_project: true, to: 'UTF-8' },
};

const encodingFixtures: Record<string, FixtureHandler> = {
  'bennu.bennu_encoding_convert_plan': CONVERT_PLAN,
};

export const panelsFixtures: Record<string, FixtureHandler> = {
  ...imberFixtures,
  ...brpFixtures,
  ...nodeFixtures,
  ...encodingFixtures,
};
