/** Gallery scene overrides and extra scenes for the "panels" feature area — see gallery/registry.ts. */
import { mount } from 'svelte';
import type { SceneOverride, ExtraScene } from './scenes';
import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
import { imberStore } from '$lib/stores/corvus/imber.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuBrpStore } from '$lib/stores/bennu/brp.svelte';
import { brpDriveStore } from '$lib/stores/bennu/brp-drive.svelte';
import type { BrpTab } from '$lib/stores/bennu/brp.svelte';
import { TAB_ID, REPO_PATH } from '../fixtures/corvus';
import { NODE_ROOT } from '../fixtures/panels';

const noop = () => {};
const DEMO = 'demo://GestionaleAtti';
const BANDO = `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Same box the bennu panels use: a fixed-size frame, since the gallery gives a panel no parent height. */
const framed = (
  load: () => Promise<{ default: any }>,
  w: number,
  h: number,
): ExtraScene['load'] => async () => {
  const inner = (await load()).default;
  const Frame = (anchor: Node, props: Record<string, unknown>) => {
    const box = document.createElement('div');
    box.style.cssText = `width:${w}px;height:${h}px;margin:24px;display:flex;flex-direction:column;overflow:hidden;background:var(--bg-base);border:1px solid var(--border-subtle, #333);border-radius:var(--radius-lg, 8px)`;
    anchor.parentNode?.insertBefore(box, anchor);
    mount(inner, { target: box, props });
  };
  return { default: Frame as any };
};

// ── Imber in Corvus ─────────────────────────────────────────────────────────────────────────────

let imberSeeded = false;
/** A repo tab, the panel opened, and one user request sent: the mock backend then streams the reply. */
async function seedImber(): Promise<void> {
  if (imberSeeded) return;
  imberSeeded = true;
  if (!tabsStore.tabs.some((t) => t.id === TAB_ID)) {
    tabsStore.addTab({ tab_id: TAB_ID, path: REPO_PATH, name: 'arbor', current_branch: 'feature/bennu', is_bare: false, is_empty: false });
  }
  tabsStore.setActive(TAB_ID);
  imberStore.setupListeners();
  await wait(50);
  imberStore.show();
  await imberStore.loadState();
  await imberStore.send('Commit the staged changes, push the branch, then open a merge request into main.');
}

// ── Bevy Remote ─────────────────────────────────────────────────────────────────────────────────

const INPUT_SCRIPT = JSON.stringify({
  version: 1,
  steps: [
    { at: 0, call: 'key', args: { key: 'Escape' } },
    { at: 420, call: 'mouse', args: { action: 'click', x: 640, y: 360 } },
    { at: 900, call: 'text', args: { text: 'Aldric' } },
    { at: 1500, call: 'key', args: { key: 'Enter' } },
    { at: 2600, call: 'key', args: { key: 'KeyW', action: 'down' } },
    { at: 4100, call: 'key', args: { key: 'KeyW', action: 'up' } },
    { at: 4300, call: 'gamepad', args: { button: 'South' } },
  ],
});

/** What a Bevy Remote scene opens on: the bottom panel's view, and what is selected in the sidebar tree. */
interface BrpSeed {
  tab: BrpTab;
  entity?: number;
  resource?: string;
  state?: string;
  /** A filter typed into the tree's box. */
  search?: string;
  /** Open the three roots and the first levels of the hierarchy (the real default is everything closed). */
  open?: boolean;
}

/** Connect to the mock game and select what the scene is about. */
const seedBrp = (o: BrpSeed) => async () => {
  await bennuBrpStore.connect();
  await bennuBrpStore.loadResources();
  await bennuBrpStore.loadStates();
  if (o.open) {
    (['entities', 'resources', 'states'] as const).forEach((r) => bennuBrpStore.openRoot(r, true));
    await bennuBrpStore.toggle(103, true);
    await bennuBrpStore.toggle(110, true);
    await bennuBrpStore.toggle(120, true);
  }
  if (o.entity !== undefined) await bennuBrpStore.select(o.entity);
  if (o.search) bennuBrpStore.setSearch(o.search);
  if (o.resource) await bennuBrpStore.selectResource(o.resource);
  if (o.state) await bennuBrpStore.selectState(o.state);
  if (o.tab === 'input') brpDriveStore.loadScript(INPUT_SCRIPT);
  bennuBrpStore.setTab(o.tab);
  // The panel resets the drive store when it mounts, which in the mock lands after the first answers
  // (a real IPC round trip is slower): ask again once it has settled.
  setTimeout(() => {
    void brpDriveStore.loadInfo(true);
    if (o.tab === 'screen') void brpDriveStore.capture(900);
  }, 400);
};

/** The bottom panel: the detail of the selection, or one of the game-wide views. */
const brpScene = (suffix: string, seed: BrpSeed, settle = 900): ExtraScene => ({
  id: `bennu/brp/BennuBrpPanel${suffix}`,
  load: framed(() => import('$lib/components/bennu/brp/BennuBrpPanel.svelte'), 1000, 460),
  setup: seedBrp(seed),
  settle,
});

/** The right sidebar: the tree with its three roots. */
const treeScene = (suffix: string, seed: BrpSeed): ExtraScene => ({
  id: `bennu/brp/BennuBevyPanel${suffix}`,
  load: framed(() => import('$lib/components/bennu/brp/BennuBevyPanel.svelte'), 340, 620),
  setup: seedBrp(seed),
  settle: 900,
});

const SCORE = 'dungeon_crawl::score::Score';
const APP_STATE = 'dungeon_crawl::app::AppState';

// ── Scenes ──────────────────────────────────────────────────────────────────────────────────────

export const panelsOverrides: Record<string, SceneOverride> = {
  'bennu/encoding/BennuEncodingConvertModal': {
    setup: () => { projectStore.loadDemo(); },
    props: () => ({ onClose: noop }),
    settle: 1000,
  },
  'bennu/encoding/BennuReinterpretModal': {
    setup: async () => { projectStore.loadDemo(); await projectStore.openFile(BANDO); },
    props: () => ({ onClose: noop }),
    settle: 800,
  },
};

export const panelsExtras: ExtraScene[] = [
  {
    id: 'shared/internal/imber/ImberChatPanel',
    load: framed(() => import('./ImberChatDemo.svelte'), 460, 640),
    settle: 600,
  },
  {
    id: 'corvus/imber/ImberPanel',
    load: framed(() => import('$lib/components/corvus/imber/ImberPanel.svelte'), 460, 780),
    setup: seedImber,
    settle: 2500,
  },
  treeScene('[tree]', { tab: 'detail' }),
  treeScene('[open]', { tab: 'detail', open: true, entity: 121 }),
  treeScene('[filter]', { tab: 'detail', search: 'gob' }),
  brpScene('[entities]', { tab: 'detail', open: true, entity: 121 }),
  brpScene('[color]', { tab: 'detail', open: true, entity: 105 }),
  brpScene('[described]', { tab: 'detail', open: true, entity: 110 }),
  brpScene('[states]', { tab: 'detail', open: true, state: APP_STATE }),
  brpScene('[resources]', { tab: 'detail', open: true, resource: SCORE }),
  brpScene('[screen]', { tab: 'screen' }),
  brpScene('[input]', { tab: 'input' }),
  brpScene('[stats]', { tab: 'stats' }, 1500),
  {
    id: 'bennu/BennuNodePanel',
    load: framed(() => import('$lib/components/bennu/BennuNodePanel.svelte'), 380, 640),
    setup: async () => { await projectStore.openProject(NODE_ROOT); },
    settle: 1200,
  },
];
