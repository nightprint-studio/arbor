import { getActivityBarConfig, setActivityBarConfig } from '$lib/ipc/config';
import type { ActivityBarItemConfig } from '$lib/types/config';

// ── Built-in item definitions ─────────────────────────────────────────────────

export interface BuiltinItem {
  id: string;
  label: string;
  mandatory: boolean;
  section: 'top' | 'bottom';
}

/** Canonical ordered list of built-in top-section items (sidebar toggles). */
export const BUILTIN_TOP: BuiltinItem[] = [
  { id: 'branches',  label: 'Branches & Stashes',    mandatory: true,  section: 'top' },
  { id: 'gitflow',   label: 'Git Flow',               mandatory: false, section: 'top' },
  { id: 'mr',        label: 'Pull / Merge Requests',  mandatory: false, section: 'top' },
  { id: 'issues',    label: 'Issues',                 mandatory: false, section: 'top' },
  { id: 'files',     label: 'Files',                  mandatory: false, section: 'top' },
  { id: 'reflog',    label: 'Reflog',                 mandatory: false, section: 'top' },
  { id: 'stats',     label: 'Repository Statistics',  mandatory: false, section: 'top' },
  { id: 'security',  label: 'Security',               mandatory: false, section: 'top' },
  { id: 'studio',    label: 'Studio (RON/JSON/TOML)',  mandatory: false, section: 'top' },
];

/** Canonical ordered list of built-in bottom-section items (panel toggles). */
export const BUILTIN_BOTTOM: BuiltinItem[] = [
  { id: 'pipelines', label: 'Pipelines',    mandatory: false, section: 'bottom' },
  { id: 'stage',     label: 'Stage & Commit', mandatory: true,  section: 'bottom' },
  { id: 'detail',    label: 'Commit Detail',  mandatory: true,  section: 'bottom' },
  { id: 'terminal',  label: 'Terminal',       mandatory: false, section: 'bottom' },
];

/** IDs that can never be hidden. */
export const MANDATORY_IDS = new Set(['branches', 'stage', 'detail']);

// ── Types ─────────────────────────────────────────────────────────────────────

/** A resolved display item (built-in or plugin) with visibility info. */
export interface ActivityBarDisplayItem extends ActivityBarItemConfig {
  label: string;
  mandatory: boolean;
  /** 'builtin' | 'plugin' */
  kind: 'builtin' | 'plugin';
  section: 'top' | 'bottom';
}

// ── Store ─────────────────────────────────────────────────────────────────────

function createActivityBarConfigStore() {
  // Raw saved config (ordered item arrays per section).
  // Left bar:
  let topItems    = $state<ActivityBarItemConfig[]>([]);
  let bottomItems = $state<ActivityBarItemConfig[]>([]);
  // Right bar — plugins only, no built-ins.
  let rightTopItems    = $state<ActivityBarItemConfig[]>([]);
  let rightBottomItems = $state<ActivityBarItemConfig[]>([]);
  let loaded      = $state(false);

  // ── Bootstrap ──────────────────────────────────────────────────────────────

  async function load() {
    try {
      const cfg = await getActivityBarConfig();
      topItems         = cfg.top_items          ?? [];
      bottomItems      = cfg.bottom_items       ?? [];
      rightTopItems    = cfg.right_top_items    ?? [];
      rightBottomItems = cfg.right_bottom_items ?? [];
    } catch {
      topItems         = [];
      bottomItems      = [];
      rightTopItems    = [];
      rightBottomItems = [];
    }
    loaded = true;
  }

  // ── Merge helpers ──────────────────────────────────────────────────────────

  /**
   * Merge saved items with the canonical defaults.
   * - Items in `saved` keep their position and visibility.
   * - Built-in items not in `saved` are appended at the end (visible by default).
   * - Plugin items in `saved` that no longer exist in `pluginIds` are kept
   *   (they might be temporarily unloaded) but won't appear in the bar.
   * - New `pluginIds` not in `saved` are appended at the end (visible).
   */
  function mergeTop(pluginIds: string[]): ActivityBarDisplayItem[] {
    return mergeSection(topItems, BUILTIN_TOP, pluginIds, 'top');
  }

  function mergeBottom(pluginIds: string[]): ActivityBarDisplayItem[] {
    return mergeSection(bottomItems, BUILTIN_BOTTOM, pluginIds, 'bottom');
  }

  /** Right bar: no built-ins, only plugin ids. Order and visibility persisted
   *  to `right_top_items` / `right_bottom_items` in the app config. */
  function mergeRightTop(pluginIds: string[]): ActivityBarDisplayItem[] {
    return mergeSection(rightTopItems, [], pluginIds, 'top');
  }
  function mergeRightBottom(pluginIds: string[]): ActivityBarDisplayItem[] {
    return mergeSection(rightBottomItems, [], pluginIds, 'bottom');
  }

  function mergeSection(
    saved: ActivityBarItemConfig[],
    builtins: BuiltinItem[],
    pluginIds: string[],
    section: 'top' | 'bottom',
  ): ActivityBarDisplayItem[] {
    const result: ActivityBarDisplayItem[] = [];
    const addedIds = new Set<string>();

    // 1. Render items in saved order.
    for (const s of saved) {
      const builtin = builtins.find(b => b.id === s.id);
      if (builtin) {
        result.push({
          id: s.id,
          visible: MANDATORY_IDS.has(s.id) ? true : s.visible,
          label: builtin.label,
          mandatory: builtin.mandatory,
          kind: 'builtin',
          section,
        });
        addedIds.add(s.id);
      } else if (pluginIds.includes(s.id)) {
        result.push({
          id: s.id,
          visible: s.visible,
          label: pluginLabelFromId(s.id),
          mandatory: false,
          kind: 'plugin',
          section,
        });
        addedIds.add(s.id);
      }
      // Unknown saved id (plugin removed?) — skip rendering but keep in storage.
    }

    // 2. Append built-ins not yet in saved.
    for (const b of builtins) {
      if (!addedIds.has(b.id)) {
        result.push({
          id: b.id,
          visible: true,
          label: b.label,
          mandatory: b.mandatory,
          kind: 'builtin',
          section,
        });
        addedIds.add(b.id);
      }
    }

    // 3. Append new plugin ids not yet in saved.
    for (const pid of pluginIds) {
      if (!addedIds.has(pid)) {
        result.push({
          id: pid,
          visible: true,
          label: pluginLabelFromId(pid),
          mandatory: false,
          kind: 'plugin',
          section,
        });
      }
    }

    return result;
  }

  function pluginLabelFromId(id: string): string {
    // id format: "plugin:{plugin_name}:{action_or_id}"
    const parts = id.split(':');
    if (parts.length >= 3) return `${parts[1]}: ${parts[2]}`;
    return id;
  }

  // ── Visibility check ───────────────────────────────────────────────────────

  /** Returns true if the given id is visible (defaults to true if not configured).
   *  Checks both left and right bar configs. */
  function isVisible(id: string): boolean {
    if (MANDATORY_IDS.has(id)) return true;
    for (const arr of [topItems, bottomItems, rightTopItems, rightBottomItems]) {
      const hit = arr.find(i => i.id === id);
      if (hit) return hit.visible;
    }
    // Not in saved config → default visible.
    return true;
  }

  // ── Persist ────────────────────────────────────────────────────────────────

  async function saveItems(
    newTop:    ActivityBarDisplayItem[],
    newBottom: ActivityBarDisplayItem[],
    newRightTop?:    ActivityBarDisplayItem[],
    newRightBottom?: ActivityBarDisplayItem[],
  ) {
    const toConfig = (items: ActivityBarDisplayItem[]): ActivityBarItemConfig[] =>
      items.map(i => ({ id: i.id, visible: MANDATORY_IDS.has(i.id) ? true : i.visible }));

    const top_items          = toConfig(newTop);
    const bottom_items       = toConfig(newBottom);
    const right_top_items    = toConfig(newRightTop    ?? []);
    const right_bottom_items = toConfig(newRightBottom ?? []);
    topItems         = top_items;
    bottomItems      = bottom_items;
    rightTopItems    = right_top_items;
    rightBottomItems = right_bottom_items;
    await setActivityBarConfig({ top_items, bottom_items, right_top_items, right_bottom_items });
  }

  return {
    get loaded()           { return loaded; },
    get topItems()         { return topItems; },
    get bottomItems()      { return bottomItems; },
    get rightTopItems()    { return rightTopItems; },
    get rightBottomItems() { return rightBottomItems; },
    load,
    mergeTop,
    mergeBottom,
    mergeRightTop,
    mergeRightBottom,
    isVisible,
    saveItems,
  };
}

export const activityBarConfigStore = createActivityBarConfigStore();
