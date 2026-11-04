/**
 * The live Bevy inspector's state — the connection, the entity tree, the selected entity, resources.
 *
 * ## What it asks the game, and how often
 *
 * BRP can neither paginate nor filter by name, so the cheapest question is a *refresh*: every
 * entity's name and parent. That is the only call that scales with the world, and it is made
 * **only while the panel is open** and **backs off while nothing changes** (1 s → 8 s, back to 1 s
 * the moment the hierarchy moves). Everything else is served from the backend's snapshot:
 * expanding a node, searching, scrolling a page.
 *
 * The selected entity is *pushed* to us by a `+watch` stream rather than polled; only a game that
 * lacks watch support (or a stream that died) falls back to a slow poll of that one entity.
 *
 * ## Connection loss
 *
 * Reconnecting is the backend's job and silent by contract; this store only mirrors the state for
 * an inline banner and shows the toasts the backend decided on (`toast` in the state event) — it
 * never decides whether to nag.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { SvelteMap, SvelteSet } from 'svelte/reactivity';
import {
  brpAncestors, brpConnect, brpDespawn, brpDisconnect, brpEntities, brpEntity, brpInsert, brpMutate,
  brpRefresh, brpRemove, brpReparent, brpResource, brpResources, brpSpawn, brpStatus, brpTypes,
  brpUnwatch, brpWatchEntity, onBrpEntity, onBrpState,
  brpTypeShape,
  type BrpEntity, type BrpEntityEvent, type BrpResourceEntry, type BrpRow, type BrpShapeInfo,
  type BrpStateEvent, type BrpStatus, type BrpTypeInfo, type BrpTypeKind,
} from '$lib/ipc/bennu/brp';
import { isEmptySearch, parseSearch } from '$lib/components/bennu/brp/brp-search';
import { currentStateName, parseStateTypes, pendingValue, type StateEntry } from '$lib/components/bennu/brp/brp-state';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

/** One level of the tree (or the flat search result): the rows loaded and how many exist. */
export interface BrpLevel {
  rows: BrpRow[];
  total: number;
  /** A filtered result: the rows that are only there as ancestors of a hit. */
  context?: number[];
}

/** What the bottom panel shows: the selected thing's detail, or one of the game-wide views. */
export type BrpTab = 'detail' | 'types' | 'console' | 'screen' | 'input' | 'stats';

/** The three roots of the sidebar tree. */
export type BrpRoot = 'entities' | 'resources' | 'states';
/** What the detail view is about. */
export type BrpFocus = 'entity' | 'resource' | 'state';

const DEFAULT_ENDPOINT = 'http://127.0.0.1:15702';
const POLL_MIN_MS = 1000;
const POLL_MAX_MS = 8000;
/** Rows fetched per tree level before a "load more" row appears. */
const LEVEL_PAGE = 500;
const SEARCH_DEBOUNCE_MS = 200;
/** Most nodes "expand all" opens: each is one request to the backend's snapshot. */
const EXPAND_ALL_LIMIT = 400;

const OFFLINE: BrpStatus = {
  connected: false, state: 'disconnected', endpoint: null, family: 'unknown', discovered: false,
  method_count: 0, type_count: 0, component_types: 0, resource_types: 0, generation: 0, non_loopback: false,
  can: {
    watch: false, spawn: false, despawn: false, insert: false, remove: false, mutate: false, reparent: false,
    resources: false, mutate_resource: false, insert_resource: false, remove_resource: false,
  },
};

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

function createBennuBrpStore() {
  // ── Connection ──
  let status = $state<BrpStatus>(OFFLINE);
  let endpoint = $state(DEFAULT_ENDPOINT);
  let connecting = $state(false);
  let connectError = $state<string | null>(null);
  /** Set while the backend is retrying: drives the inline banner, not a toast. */
  let retry = $state<{ attempt: number; max: number } | null>(null);

  // ── Navigation ──
  let tab = $state<BrpTab>('detail');
  let focus = $state<BrpFocus | null>(null);
  /** Which of the sidebar's three roots are open: none until the user opens one. */
  const rootsOpen = new SvelteSet<BrpRoot>();
  let roots = $state<BrpLevel>({ rows: [], total: 0 });
  const children = new SvelteMap<number, BrpLevel>();
  const expanded = new SvelteSet<number>();
  /** The raw search box: free text plus `has:` / `no:` filters (see `brp-search.ts`). */
  let search = $state('');
  /** Why a search found nothing when it is not simply "no match" (an unknown component name). */
  let searchNote = $state<string | null>(null);
  /** The flat result while a search or component filter is active; `null` = show the tree. */
  let flat = $state<BrpLevel | null>(null);
  let loadingTree = $state(false);

  // ── Selection ──
  let selected = $state<number | null>(null);
  let entity = $state<BrpEntity | null>(null);
  let entityError = $state<string | null>(null);
  let watching = $state(false);

  // ── Resources & types ──
  let resources = $state<BrpResourceEntry[]>([]);
  let states = $state<StateEntry[]>([]);
  let selectedState = $state<string | null>(null);
  /** Current variant name of the selected state, and the variants it can take. */
  let stateCurrent = $state<string | null>(null);
  let stateVariants = $state<{ name: string; sample: unknown }[]>([]);
  let stateError = $state<string | null>(null);
  let selectedResource = $state<string | null>(null);
  let resourceValue = $state<unknown>(undefined);
  let resourceError = $state<string | null>(null);
  let types = $state<{ items: BrpTypeInfo[]; total: number }>({ items: [], total: 0 });
  let typeQuery = $state('');
  let typeKind = $state<BrpTypeKind>('component');
  let selectedType = $state<string | null>(null);

  // ── Plumbing (not reactive) ──
  let wired = false;
  /** Panels showing the game right now (the sidebar, the detail panel): polling stops with the last one. */
  let users = 0;
  let lastGeneration = -1;
  let revision = '';
  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let polling = false;
  let pollInterval = POLL_MIN_MS;
  let busyTick = false;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let selectToken = 0;
  /** type path → editor shape, fetched once per game (a reconnect clears it). */
  const shapes = new Map<string, Promise<BrpShapeInfo | null>>();

  const isFiltering = () => !isEmptySearch(parseSearch(search));

  // ── Events ──

  function onState(ev: BrpStateEvent) {
    status = ev.status;
    retry = ev.status.state === 'reconnecting' ? { attempt: ev.attempt, max: ev.max_attempts } : null;
    if (ev.toast === 'lost') toastStore.show('Lost the connection to the game — retrying in the background.', 'warning');
    else if (ev.toast === 'regained') toastStore.show('Reconnected to the game.', 'success');
    else if (ev.toast === 'gave_up') toastStore.show('Could not reach the game again. Start it and connect manually.', 'error', 8000);

    if (ev.status.connected && ev.status.generation !== lastGeneration) {
      lastGeneration = ev.status.generation;
      void onNewWorld();
    } else if (!ev.status.connected && ev.status.state === 'disconnected') {
      stopPolling();
      clearWorld();
    }
  }

  function onEntityEvent(ev: BrpEntityEvent) {
    if (ev.generation !== status.generation || ev.entity !== selected || !entity) return;
    switch (ev.kind) {
      case 'values': {
        const components = { ...entity.components, ...ev.components };
        for (const gone of ev.removed) delete components[gone];
        entity = { ...entity, components, errors: { ...entity.errors, ...ev.errors } };
        break;
      }
      case 'shape':
        // A component came or went: re-list and re-watch, which also refetches the full set.
        void reloadEntity().then(() => startWatch(ev.entity));
        break;
      case 'error':
        // Usually the entity was despawned under us; the next refresh drops it from the tree.
        entityError = ev.message;
        watching = false;
        break;
      case 'closed':
        // Stream ended without a reconnect event (e.g. game closed the socket): poll instead.
        watching = false;
        break;
    }
  }

  async function wire() {
    if (wired) return;
    wired = true;
    await onBrpState(onState);
    await onBrpEntity(onEntityEvent);
  }

  // ── Lifecycle ──

  /** Idempotent: subscribe to the backend's pushes and pick up a connection that already exists. */
  async function init() {
    await wire();
    try {
      status = await brpStatus();
      if (status.endpoint) endpoint = status.endpoint;
      if (status.connected && status.generation !== lastGeneration) {
        lastGeneration = status.generation;
        await onNewWorld();
      }
    } catch {
      status = OFFLINE;
    }
  }

  async function connect(to?: string) {
    if (connecting) return;
    if (to !== undefined) endpoint = to;
    connecting = true;
    connectError = null;
    try {
      await wire();
      status = await brpConnect(endpoint);
      // The state event also arrives; whichever lands first does the world reset.
      if (status.generation !== lastGeneration) {
        lastGeneration = status.generation;
        await onNewWorld();
      }
    } catch (e) {
      connectError = message(e);
    } finally {
      connecting = false;
    }
  }

  async function disconnect() {
    stopPolling();
    try {
      status = await brpDisconnect();
    } catch (e) {
      toastStore.show(message(e), 'error');
    }
    clearWorld();
  }

  function clearWorld() {
    roots = { rows: [], total: 0 };
    children.clear();
    expanded.clear();
    flat = null;
    selected = null;
    entity = null;
    entityError = null;
    watching = false;
    resources = [];
    states = [];
    selectedState = null;
    stateCurrent = null;
    stateVariants = [];
    selectedResource = null;
    resourceValue = undefined;
    types = { items: [], total: 0 };
    revision = '';
    searchNote = null;
    shapes.clear();
  }

  /** A new game (first connect, or the old one restarted): nothing cached describes it. */
  async function onNewWorld() {
    const keep = selected;
    const open = [...expanded];
    clearWorld();
    await reload();
    // The same game restarted: the nodes the user had open are still where they were.
    for (const id of open) await toggle(id, true);
    void loadResources();
    void loadStates();
    void searchTypes();
    if (keep !== null) void select(keep);
    if (polling) schedule(0);
  }

  // ── Tree ──

  async function reload() {
    loadingTree = true;
    try {
      const r = await brpRefresh();
      revision = r.revision;
      await reloadVisible();
    } catch (e) {
      toastStore.show(message(e), 'error');
    } finally {
      loadingTree = false;
    }
  }

  /** Re-read exactly what is on screen: the roots, each expanded level, and a flat result if any. */
  async function reloadVisible() {
    if (isFiltering()) {
      await runSearch();
      return;
    }
    roots = await levelOf({});
    await Promise.all([...expanded].map(async (id) => {
      if (children.has(id)) children.set(id, await levelOf({ parent: id }));
    }));
  }

  async function levelOf(args: { parent?: number; offset?: number }): Promise<BrpLevel> {
    const r = await brpEntities({ ...args, limit: LEVEL_PAGE });
    return { rows: r.rows, total: r.total };
  }

  async function toggle(id: number, open: boolean) {
    if (!open) {
      expanded.delete(id);
      return;
    }
    expanded.add(id);
    if (!children.has(id)) {
      try {
        children.set(id, await levelOf({ parent: id }));
      } catch (e) {
        toastStore.show(message(e), 'error');
      }
    }
  }

  /**
   * Open every node that has children, level by level, until `EXPAND_ALL_LIMIT` nodes are open — a
   * world of ten thousand entities is not something to unfold in one click.
   */
  async function expandAll() {
    let queue = [...roots.rows];
    let opened = 0;
    while (queue.length > 0 && opened < EXPAND_ALL_LIMIT) {
      const next: BrpRow[] = [];
      for (const row of queue) {
        if (row.child_count === 0 || opened >= EXPAND_ALL_LIMIT) continue;
        await toggle(row.id, true);
        opened++;
        next.push(...(children.get(row.id)?.rows ?? []));
      }
      queue = next;
    }
    if (opened >= EXPAND_ALL_LIMIT) toastStore.show(`Opened ${opened} nodes — the rest stay closed.`, 'info');
  }

  function collapseAll() {
    expanded.clear();
  }

  /** Fetch the next page of a level (`parent` undefined = the roots). */
  async function loadMore(parent?: number) {
    const level = parent === undefined ? roots : children.get(parent);
    if (!level) return;
    const next = await levelOf({ parent, offset: level.rows.length });
    const merged = { rows: [...level.rows, ...next.rows], total: next.total };
    if (parent === undefined) roots = merged;
    else children.set(parent, merged);
  }

  function setSearch(q: string) {
    search = q;
    scheduleSearch();
  }

  function scheduleSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void runSearch(), SEARCH_DEBOUNCE_MS);
  }

  async function runSearch() {
    const parsed = parseSearch(search);
    searchNote = null;
    if (isEmptySearch(parsed)) {
      flat = null;
      roots = await levelOf({});
      return;
    }
    try {
      const [withPaths, withMissing] = await resolveComponents(parsed.has);
      const [noPaths, noMissing] = await resolveComponents(parsed.no);
      const missing = [...withMissing, ...noMissing];
      if (missing.length) {
        // A filter naming a component the game never registered can match nothing; saying so beats
        // an empty list that looks like a quiet world.
        searchNote = `Unknown component: ${missing.join(', ')}`;
        flat = { rows: [], total: 0 };
        return;
      }
      const r = await brpEntities({
        search: parsed.text || undefined,
        with: withPaths.length ? withPaths : undefined,
        without: noPaths.length ? noPaths : undefined,
        limit: LEVEL_PAGE,
        with_parents: true,
      });
      flat = { rows: r.rows, total: r.total, context: r.context };
    } catch (e) {
      toastStore.show(message(e), 'error');
    }
  }

  /** Short or full component names → registered type paths, plus the names that matched nothing. */
  async function resolveComponents(names: string[]): Promise<[string[], string[]]> {
    const found: string[] = [];
    const missing: string[] = [];
    for (const name of names) {
      if (name.includes('::')) { found.push(name); continue; }
      const hits = (await brpTypes({ kind: 'component', query: name, limit: 20 })).items;
      const exact = hits.find((h) => h.short_path.toLowerCase() === name.toLowerCase());
      const pick = exact ?? (hits.length === 1 ? hits[0] : undefined);
      if (pick) found.push(pick.type_path); else missing.push(name);
    }
    return [found, missing];
  }

  /** Leave search and show `id` in the tree: expand every ancestor, then select it. */
  async function reveal(id: number) {
    search = '';
    searchNote = null;
    flat = null;
    rootsOpen.add('entities');
    roots = await levelOf({});
    for (const a of await brpAncestors(id)) await toggle(a, true);
    await select(id);
  }

  // ── Polling ──

  function schedule(ms: number) {
    if (pollTimer) clearTimeout(pollTimer);
    if (polling) pollTimer = setTimeout(() => void tick(), ms);
  }

  async function tick() {
    if (!polling || busyTick) return;
    busyTick = true;
    try {
      // A hidden window or a game that is not there has nothing to refresh.
      if (status.connected && !document.hidden) {
        const r = await brpRefresh();
        if (r.revision !== revision) {
          revision = r.revision;
          pollInterval = POLL_MIN_MS;
          await reloadVisible();
        } else {
          pollInterval = Math.min(POLL_MAX_MS, Math.round(pollInterval * 1.5));
        }
        // No push available for the selection: poll just that entity, on the same backoff.
        if (selected !== null && !watching) await reloadEntity(true);
      }
    } catch {
      pollInterval = Math.min(POLL_MAX_MS, pollInterval * 2);
    } finally {
      busyTick = false;
      schedule(pollInterval);
    }
  }

  /** Start refreshing while the panel is visible. Safe to call repeatedly. */
  function startPolling() {
    if (polling) return;
    polling = true;
    pollInterval = POLL_MIN_MS;
    schedule(status.connected ? POLL_MIN_MS : 0);
  }

  function stopPolling() {
    polling = false;
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = null;
  }

  /** The panel was hidden: stop asking, and stop the game pushing frames nobody reads. */
  function suspend() {
    users = Math.max(0, users - 1);
    if (users > 0) return;
    stopPolling();
    watching = false;
    void brpUnwatch().catch(() => {});
  }

  /** The panel is visible again: catch up, and re-subscribe the selected entity. */
  function resume() {
    users++;
    startPolling();
    if (status.connected && selected !== null) {
      void reloadEntity().then(() => (status.can.watch && selected !== null ? startWatch(selected) : undefined));
    }
  }

  // ── Selection ──

  async function select(id: number | null) {
    const token = ++selectToken;
    selected = id;
    if (id !== null) focus = 'entity';
    entityError = null;
    watching = false;
    void brpUnwatch().catch(() => {});
    if (id === null) {
      entity = null;
      return;
    }
    await reloadEntity();
    if (token !== selectToken) return;
    if (status.can.watch && !entityError) await startWatch(id);
  }

  async function startWatch(id: number) {
    try {
      await brpWatchEntity(id);
      watching = id === selected;
    } catch {
      watching = false;
    }
  }

  /** Re-read the selected entity. `quiet` = a poll: don't flash an error for a transient miss. */
  async function reloadEntity(quiet = false) {
    const id = selected;
    if (id === null) return;
    try {
      const fresh = await brpEntity(id);
      if (id !== selected) return;
      // Skip the assignment when nothing moved, so an idle poll does not re-render the inspector.
      if (!entity || entity.id !== id || JSON.stringify(entity.components) !== JSON.stringify(fresh.components)
          || JSON.stringify(entity.errors) !== JSON.stringify(fresh.errors) || entity.name !== fresh.name) {
        entity = fresh;
      }
      entityError = null;
    } catch (e) {
      if (id !== selected) return;
      if (!quiet || !entity) entityError = message(e);
    }
  }

  // ── Writes ──

  /** Run a write, toast the game's own message if it refuses, and report whether it went through. */
  async function attempt(what: string, run: () => Promise<unknown>): Promise<boolean> {
    try {
      await run();
      return true;
    } catch (e) {
      toastStore.show(`${what}: ${message(e)}`, 'error', 6000);
      return false;
    }
  }

  async function afterWrite() {
    // A watch frame carries the new value; without one, read it back.
    if (!watching) await reloadEntity();
  }

  async function mutateComponent(typePath: string, path: string, value: unknown) {
    const id = selected;
    if (id === null) return;
    const ok = await attempt('Cannot set value', () => brpMutate({ entity: id, type_path: typePath, path, value }));
    if (ok) await afterWrite(); else await reloadEntity();
  }

  /** Replace a component whole (raw-JSON edit) or add it. */
  async function insertComponent(typePath: string, value: unknown): Promise<boolean> {
    const id = selected;
    if (id === null) return false;
    const ok = await attempt('Cannot insert component', () => brpInsert({ entity: id, type_path: typePath, value }));
    if (ok) {
      // A new component changes the set, which a value-only watch cannot see.
      await reloadEntity();
      if (watching) await startWatch(id);
    }
    return ok;
  }

  async function removeComponent(typePath: string) {
    const id = selected;
    if (id === null) return;
    const ok = await attempt('Cannot remove component', () => brpRemove({ entity: id, type_path: typePath }));
    if (ok) {
      await reloadEntity();
      if (watching) await startWatch(id);
    }
  }

  async function spawn(components: Record<string, unknown> = {}) {
    let id: number | null = null;
    const ok = await attempt('Cannot spawn', async () => { id = await brpSpawn(components); });
    if (!ok || id === null) return;
    await reload();
    await reveal(id);
  }

  async function despawn(id: number) {
    const ok = await attempt('Cannot despawn', () => brpDespawn(id));
    if (!ok) return;
    if (selected === id) await select(null);
    await reload();
  }

  async function reparent(id: number, parent?: number) {
    if (await attempt('Cannot reparent', () => brpReparent(id, parent))) await reload();
  }

  // ── Resources ──

  async function loadResources() {
    if (!status.can.resources) {
      resources = [];
      return;
    }
    try {
      resources = await brpResources();
    } catch (e) {
      toastStore.show(message(e), 'error');
    }
  }

  async function selectResource(typePath: string | null) {
    selectedResource = typePath;
    if (typePath !== null) focus = 'resource';
    resourceError = null;
    resourceValue = undefined;
    if (typePath !== null) await reloadResource();
  }

  async function reloadResource() {
    const path = selectedResource;
    if (path === null) return;
    try {
      const r = await brpResource(path);
      if (path === selectedResource) resourceValue = r.value;
      resourceError = null;
    } catch (e) {
      if (path === selectedResource) resourceError = message(e);
    }
  }

  async function mutateResource(typePath: string, path: string, value: unknown) {
    const ok = await attempt('Cannot set value', () => brpMutate({ type_path: typePath, path, value }));
    await reloadResource();
    return ok;
  }

  async function replaceResource(typePath: string, value: unknown) {
    const ok = await attempt('Cannot replace resource', () => brpInsert({ type_path: typePath, value }));
    await reloadResource();
    return ok;
  }

  async function removeResource(typePath: string) {
    if (!(await attempt('Cannot remove resource', () => brpRemove({ type_path: typePath })))) return;
    await selectResource(null);
    await loadResources();
  }

  // ── Types ──

  /** The editor shape of a type, cached; `null` when the game gave none (the editor goes raw). */
  function shapeOf(typePath: string): Promise<BrpShapeInfo | null> {
    let p = shapes.get(typePath);
    if (!p) {
      p = brpTypeShape(typePath).catch(() => null);
      shapes.set(typePath, p);
    }
    return p;
  }

  async function searchTypes(query?: string, kind?: BrpTypeKind) {
    if (query !== undefined) typeQuery = query;
    if (kind !== undefined) typeKind = kind;
    try {
      types = await brpTypes({ kind: typeKind, query: typeQuery, limit: 200 });
    } catch {
      types = { items: [], total: 0 };
    }
  }

  /** What the tree knows an entity to be (`camera`, `light`, …), from whichever loaded level lists it. */
  function kindOf(id: number): string | null {
    const levels = [roots, ...(flat ? [flat] : []), ...children.values()];
    for (const level of levels) {
      const row = level.rows.find((r) => r.id === id);
      if (row) return row.kind ?? null;
    }
    return null;
  }

  // ── States ──

  /** Find the game's state machines: every `State<T>` resource in the registry, with its `NextState<T>` if any. */
  async function loadStates() {
    try {
      const page = await brpTypes({ kind: 'resource', query: 'State<', limit: 400 });
      states = parseStateTypes(page.items.map((t) => t.type_path));
    } catch {
      states = [];
    }
  }

  async function selectState(enumPath: string | null) {
    selectedState = enumPath;
    stateError = null;
    stateCurrent = null;
    stateVariants = [];
    if (enumPath === null) return;
    focus = 'state';
    const entry = states.find((e) => e.enumPath === enumPath);
    if (!entry) return;
    try {
      const [value, shape] = await Promise.all([brpResource(entry.statePath), brpTypeShape(enumPath)]);
      if (enumPath !== selectedState) return;
      stateCurrent = currentStateName(value.value);
      stateVariants = shape.shape.kind === 'enum' ? shape.shape.variants.map((v) => ({ name: v.name, sample: v.sample })) : [];
    } catch (e) {
      if (enumPath === selectedState) stateError = message(e);
    }
  }

  /** Ask the game to move the state machine to `variant` (it takes effect on its next state-transition run). */
  async function setState(variant: string): Promise<boolean> {
    const entry = states.find((e) => e.enumPath === selectedState);
    const sample = stateVariants.find((v) => v.name === variant)?.sample;
    if (!entry?.nextPath || sample === undefined) {
      stateError = 'This game did not register NextState for this state, so it cannot be changed from here.';
      return false;
    }
    const ok = await attempt('set state', () => brpInsert({ type_path: entry.nextPath!, value: pendingValue(sample) }));
    if (ok) setTimeout(() => void selectState(entry.enumPath), 300);
    return ok;
  }

  function openRoot(root: BrpRoot, open: boolean) {
    if (open) rootsOpen.add(root);
    else rootsOpen.delete(root);
  }

  return {
    get status() { return status; },
    get endpoint() { return endpoint; },
    set endpoint(v: string) { endpoint = v; },
    get connecting() { return connecting; },
    get connectError() { return connectError; },
    get retry() { return retry; },
    get connected() { return status.connected; },

    get tab() { return tab; },
    setTab(t: BrpTab) { tab = t; },
    get focus() { return focus; },
    get rootsOpen() { return rootsOpen; },
    openRoot,
    get states() { return states; },
    get selectedState() { return selectedState; },
    get stateCurrent() { return stateCurrent; },
    get stateVariants() { return stateVariants; },
    get stateError() { return stateError; },
    loadStates, selectState, setState,
    get roots() { return roots; },
    get children() { return children; },
    get expanded() { return expanded; },
    get search() { return search; },
    get searchNote() { return searchNote; },
    get flat() { return flat; },
    get loadingTree() { return loadingTree; },

    get selected() { return selected; },
    /** The kind of the selected entity, for its icon. */
    get entityKind() { return selected === null ? null : kindOf(selected); },
    get entity() { return entity; },
    get entityError() { return entityError; },
    get watching() { return watching; },

    get resources() { return resources; },
    get selectedResource() { return selectedResource; },
    get resourceValue() { return resourceValue; },
    get resourceError() { return resourceError; },
    get types() { return types; },
    get typeQuery() { return typeQuery; },
    get typeKind() { return typeKind; },
    get selectedType() { return selectedType; },
    selectType(path: string | null) { selectedType = path; },

    init, connect, disconnect, reload,
    toggle, expandAll, collapseAll, loadMore, setSearch, reveal, select, reloadEntity,
    suspend, resume,
    mutateComponent, insertComponent, removeComponent, spawn, despawn, reparent,
    loadResources, selectResource, reloadResource, mutateResource, replaceResource, removeResource,
    searchTypes, shapeOf,
  };
}

export const bennuBrpStore = createBennuBrpStore();
