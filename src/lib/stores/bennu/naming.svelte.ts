/**
 * Bennu naming-convention store — the per-repo `[naming]` section, plus the BE's catalog of packs,
 * targets and conventions.
 *
 * Owns the *loaded* config for the open project and the *draft* the settings section edits. They
 * are separate on purpose: a settings screen the user can cancel out of must not have written
 * anything, and the diagnostics the editor is drawing come from the loaded one. `apply()` is the
 * only thing that writes — and writing bumps `revision`, which the editor's validation effect
 * watches so squiggles follow a rule change on the next debounce instead of on the next reopen.
 *
 * The catalog is fetched once per session: it is static data compiled into the BE.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen } from '@tauri-apps/api/event';
import {
  cancelNamingFix as ipcCancelFix,
  emptyNamingConfig,
  getNamingConfig as ipcGet,
  namingCatalog as ipcCatalog,
  namingFixPlan as ipcFixPlan,
  setNamingConfig as ipcSet,
  type FixProgress,
  type NamingCatalog,
  type NamingFixPlan,
  type NamingConfig,
  type NamingConvention,
  type NamingRules,
  type NamingTarget,
} from '$lib/ipc/bennu/naming';

/** Deep-copy a rules-by-pack map, so a draft never shares a nested object with the loaded config. */
function cloneRules(rules: Record<string, NamingRules>): Record<string, NamingRules> {
  return Object.fromEntries(Object.entries(rules).map(([k, v]) => [k, { ...v }]));
}

/** Deep-copy a config, so a draft never aliases the loaded one. */
function clone(config: NamingConfig): NamingConfig {
  return {
    enabled: config.enabled,
    ignore: [...config.ignore],
    rules: cloneRules(config.rules),
    // Defaulted, not assumed: a config decoded from an older file has no `overrides` key, and
    // spreading `undefined` into the draft would break every reader of the list.
    overrides: (config.overrides ?? []).map((o) => ({
      name: o.name,
      paths: [...o.paths],
      rules: cloneRules(o.rules),
    })),
  };
}

/** Replace one element of a list through `patch`, leaving the rest identical. */
function patchAt<T>(list: T[], index: number, patch: (item: T) => T): T[] {
  return list.map((item, i) => (i === index ? patch(item) : item));
}

function createBennuNamingStore() {
  let catalog = $state<NamingCatalog | null>(null);
  // The root the catalog's `present` flags were computed for — see `loadCatalog`.
  let catalogRoot = $state<string | null>(null);
  let loadedRoot = $state<string | null>(null);
  let loaded = $state<NamingConfig>(emptyNamingConfig());
  let draft = $state<NamingConfig>(emptyNamingConfig());
  let saving = $state(false);
  // The bulk fix, from "asked for" to "applied or dismissed". Held here rather than in a
  // component so the palette can start one and the modal that reviews it is just a renderer —
  // which is what lets the modal open before the work rather than after it.
  let fixOpen = $state(false);
  let pendingFix = $state<NamingFixPlan | null>(null);
  let planningFix = $state(false);
  let fixProgress = $state<FixProgress | null>(null);
  /** What the pending plan covers, for the modal's title. */
  let fixScope = $state<'file' | 'project'>('file');

  // Attached on the first fix and kept: a listener costs nothing while no fix is running, and
  // re-attaching per run is a race against the first event the backend emits.
  let progressAttached = false;
  async function attachProgress() {
    if (progressAttached) return;
    progressAttached = true;
    try {
      await listen<FixProgress>('arbor://bennu/naming-fix-progress', (e) => {
        if (planningFix) fixProgress = e.payload;
      });
    } catch {
      progressAttached = false;
    }
  }
  // Bumped on every successful write — what the editor's validation effect watches.
  let revision = $state(0);

  return {
    get catalog() { return catalog; },
    get config() { return loaded; },
    get draft() { return draft; },
    get saving() { return saving; },
    get revision() { return revision; },
    /** Whether the draft differs from what is on disk — what gates the Apply button. */
    get dirty() { return JSON.stringify(draft) !== JSON.stringify(loaded); },

    /**
     * Fetch the catalog for `root`.
     *
     * Re-fetched when the project changes rather than cached for the session: the packs are static,
     * but *which of them this project contains* is not, and showing a Rust column on a Java project
     * because a Rust one was open earlier is the bug this parameter exists to prevent.
     */
    async loadCatalog(root: string | null) {
      if (catalog && catalogRoot === root) return;
      try {
        catalog = await ipcCatalog(root ?? undefined);
        catalogRoot = root;
      } catch {
        catalog = null;
      }
    },

    /** Load `root`'s section and seed the draft from it. */
    async load(root: string) {
      try {
        loaded = await ipcGet(root);
      } catch {
        loaded = emptyNamingConfig();
      }
      loadedRoot = root;
      draft = clone(loaded);
    },

    /** Throw the draft away and start again from what is on disk. */
    revert() {
      draft = clone(loaded);
    },

    setEnabled(on: boolean) {
      draft = { ...draft, enabled: on };
    },

    setIgnore(globs: string[]) {
      draft = { ...draft, ignore: globs };
    },

    /** Set one target's convention for one pack. `"any"` is stored, not deleted, so the settings
     *  screen shows the explicit choice the user made rather than an empty cell. */
    setConvention(packId: string, target: NamingTarget, convention: NamingConvention) {
      const rules = { ...(draft.rules[packId] ?? {}), [target]: convention };
      draft = { ...draft, rules: { ...draft.rules, [packId]: rules } };
    },

    /** Fill a pack's rules with the community standard the BE offers. Never applied on its own —
     *  this is what the "Use the standard convention" button does. */
    adoptStandard(packId: string) {
      const pack = catalog?.packs.find((p) => p.id === packId);
      if (!pack) return;
      draft = { ...draft, rules: { ...draft.rules, [packId]: { ...pack.standard } } };
    },

    /** Switch every target of a pack back off. */
    clearPack(packId: string) {
      draft = { ...draft, rules: { ...draft.rules, [packId]: {} } };
    },

    // ── path-scoped overrides ────────────────────────────────────────────────
    //
    // A list, not a map: two overrides can claim the same file and the later one wins, so the
    // order is part of what the user configured and an index is how a row addresses itself.

    /** Append an empty override. It claims nothing until a path is typed into it. */
    addOverride() {
      draft = { ...draft, overrides: [...draft.overrides, { name: '', paths: [], rules: {} }] };
    },

    removeOverride(index: number) {
      draft = { ...draft, overrides: draft.overrides.filter((_, i) => i !== index) };
    },

    setOverrideName(index: number, name: string) {
      draft = { ...draft, overrides: patchAt(draft.overrides, index, (o) => ({ ...o, name })) };
    },

    setOverridePaths(index: number, paths: string[]) {
      draft = { ...draft, overrides: patchAt(draft.overrides, index, (o) => ({ ...o, paths })) };
    },

    /** Set one target inside one override. Storing `"any"` is the POINT here — that is how a
     *  subtree turns a rule off without touching the project-wide one. */
    setOverrideConvention(
      index: number,
      packId: string,
      target: NamingTarget,
      convention: NamingConvention,
    ) {
      draft = {
        ...draft,
        overrides: patchAt(draft.overrides, index, (o) => ({
          ...o,
          rules: { ...o.rules, [packId]: { ...(o.rules[packId] ?? {}), [target]: convention } },
        })),
      };
    },

    get fixOpen() { return fixOpen; },
    get pendingFix() { return pendingFix; },
    get planningFix() { return planningFix; },
    get fixProgress() { return fixProgress; },
    get fixScope() { return fixScope; },

    /**
     * Plan the bulk fix for one file, or — with no `file` — for the whole project.
     *
     * Opens the review **immediately**, in its working state, and fills it in when the plan
     * arrives. Planning a project can take a while; a command that appears to do nothing for a
     * minute and then produces a modal is indistinguishable from one that has hung, which is
     * exactly what the first cut of this felt like.
     *
     * Nothing is written: the plan is held for review, and whoever renders it applies the edits.
     */
    async planFix(root: string, file?: string, source?: string): Promise<NamingFixPlan | null> {
      if (planningFix) return null;
      planningFix = true;
      fixOpen = true;
      pendingFix = null;
      fixProgress = null;
      fixScope = file ? 'file' : 'project';
      await attachProgress();
      try {
        const plan = await ipcFixPlan(root, file, source);
        pendingFix = plan;
        return plan;
      } catch {
        pendingFix = null;
        fixOpen = false;
        return null;
      } finally {
        planningFix = false;
        fixProgress = null;
      }
    },

    /**
     * Ask the backend to stop planning.
     *
     * The request still resolves — with whatever it had — so the modal shows a partial plan the
     * user can apply or throw away, rather than nothing at all.
     */
    cancelFix(root: string) {
      if (!planningFix) return;
      void ipcCancelFix(root);
    },

    /** Close the review, dropping whatever it held. */
    dismissFix() {
      pendingFix = null;
      fixOpen = false;
      fixProgress = null;
    },

    /** Persist the draft. Returns whether it was written. */
    async apply(): Promise<boolean> {
      const root = loadedRoot;
      if (!root || saving) return false;
      saving = true;
      try {
        await ipcSet(root, draft);
        loaded = clone(draft);
        revision += 1;
        return true;
      } catch {
        return false;
      } finally {
        saving = false;
      }
    },
  };
}

export const bennuNamingStore = createBennuNamingStore();
