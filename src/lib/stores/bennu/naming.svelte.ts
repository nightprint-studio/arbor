/**
 * Bennu naming-convention store — two documents of the same shape, the BE's catalog of packs,
 * targets and conventions, and the bulk fix.
 *
 * ## Two documents, one set of operations
 *
 * The **profile** holds your answer for every project; a **project** states only what it says
 * differently, and the backend merges them. They are edited by two different screens but there is
 * exactly one set of operations on them — set a convention, adopt a standard, add an exception —
 * so {@link createNamingDocument} is written once and instantiated twice. The screens take a
 * document as a prop and never name which one they are on: that is the whole reason the same grid
 * can serve Settings and Project Configuration without a fork.
 *
 * Each document keeps its *loaded* copy apart from the *draft* the screen edits: a settings screen
 * the user can cancel out of must not have written anything, and the diagnostics the editor is
 * drawing come from the loaded one. `apply()` is the only thing that writes — and writing bumps
 * `revision`, which the editor's validation effect watches so squiggles follow a rule change on
 * the next debounce instead of on the next reopen.
 *
 * The catalog is shared: it is static data compiled into the BE, re-fetched only when the project
 * changes because *which packs this project contains* is not static.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen } from '@tauri-apps/api/event';
import {
  cancelNamingFix as ipcCancelFix,
  emptyNamingConfig,
  getNamingConfig as ipcGet,
  getNamingDefaults as ipcGetDefaults,
  namingCatalog as ipcCatalog,
  namingFixPlan as ipcFixPlan,
  setNamingConfig as ipcSet,
  setNamingDefaults as ipcSetDefaults,
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
    // Defaulted rather than assumed: a section written before the profile level existed has no
    // `inherit` key, and the backend's default for a missing one is `true`.
    inherit: config.inherit ?? true,
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

/** Where a document reads and writes itself. The only thing the two levels differ by. */
interface NamingIo {
  read: () => Promise<NamingConfig>;
  write: (config: NamingConfig) => Promise<void>;
  /** Whether there is anything to read yet — a project document with no project open has not. */
  ready: () => boolean;
}

/**
 * One editable naming document.
 *
 * Everything a screen does to a set of rules, over whichever of the two levels it was handed.
 */
function createNamingDocument(io: NamingIo, onWrite: () => void) {
  let loaded = $state<NamingConfig>(emptyNamingConfig());
  let draft = $state<NamingConfig>(emptyNamingConfig());
  let saving = $state(false);
  /** In flight, so two screens mounting together read once. */
  let reading: Promise<void> | null = null;
  /** Whether this document has ever been read. What makes `load()` idempotent. */
  let everRead = false;

  async function read(): Promise<void> {
    try {
      loaded = await io.read();
    } catch {
      loaded = emptyNamingConfig();
    }
    everRead = true;
    draft = clone(loaded);
  }

  /** The read, at most once per document — see `load` below for why. */
  async function loadOnce(): Promise<void> {
    if (!io.ready() || everRead) return;
    reading ??= read().finally(() => {
      reading = null;
    });
    await reading;
  }

  return {
    get config() {
      return loaded;
    },
    get draft() {
      return draft;
    },
    get saving() {
      return saving;
    },
    /** Whether the draft differs from what is on disk — what gates an Apply button. */
    get dirty() {
      return JSON.stringify(draft) !== JSON.stringify(loaded);
    },

    /**
     * Read it from disk and seed the draft — **once**.
     *
     * Idempotent on purpose: a screen calls this from an effect, and an effect re-runs. A read that
     * re-seeded every time would throw away conventions somebody was halfway through choosing the
     * moment anything else on the page moved. {@link reload} is the explicit re-read.
     */
    load: loadOnce,

    /** Read it again, discarding the draft — what a change of project means for this document. */
    async reload(): Promise<void> {
      everRead = false;
      await loadOnce();
    },

    /** Throw the draft away and start again from what is on disk. */
    revert() {
      draft = clone(loaded);
    },

    setEnabled(on: boolean) {
      draft = { ...draft, enabled: on };
    },

    /** Whether this document starts from the level above it. Meaningless on the profile. */
    setInherit(on: boolean) {
      draft = { ...draft, inherit: on };
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

    /** Fill a pack's rules with a standard. Never applied on its own — this is what the "Use the
     *  standard convention" button does. */
    adoptRules(packId: string, rules: NamingRules) {
      draft = { ...draft, rules: { ...draft.rules, [packId]: { ...rules } } };
    },

    /** Switch every target of a pack back off. */
    clearPack(packId: string) {
      draft = { ...draft, rules: { ...draft.rules, [packId]: {} } };
    },

    /** Drop a pack's rules entirely, so the level above answers for it again. Distinct from
     *  {@link clearPack}, which states "no rule" and therefore overrides an inherited one. */
    unsetPack(packId: string) {
      const rules = { ...draft.rules };
      delete rules[packId];
      draft = { ...draft, rules };
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

    /** Persist the draft. Returns whether it was written. */
    async apply(): Promise<boolean> {
      if (saving || !io.ready()) return false;
      saving = true;
      try {
        await io.write(draft);
        loaded = clone(draft);
        onWrite();
        return true;
      } catch {
        return false;
      } finally {
        saving = false;
      }
    },
  };
}

/** One editable naming document, as a screen receives it. */
export type NamingDocument = ReturnType<typeof createNamingDocument>;

function createBennuNamingStore() {
  let catalog = $state<NamingCatalog | null>(null);
  // The root the catalog's `present` flags were computed for — see `loadCatalog`.
  let catalogRoot = $state<string | null>(null);
  let loadedRoot = $state<string | null>(null);
  // Bumped on every successful write to either document — what the editor's validation effect
  // watches. The profile counts: its rules reach every project, including the one on screen.
  let revision = $state(0);
  const bump = () => {
    revision += 1;
  };

  const project = createNamingDocument(
    {
      read: () => ipcGet(loadedRoot ?? ''),
      write: (config) => ipcSet(loadedRoot ?? '', config),
      ready: () => !!loadedRoot,
    },
    bump,
  );
  const profile = createNamingDocument(
    { read: ipcGetDefaults, write: ipcSetDefaults, ready: () => true },
    bump,
  );

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

  return {
    get catalog() { return catalog; },
    get revision() { return revision; },

    /** The open project's own section — what it states differently. */
    get project() { return project; },
    /** The profile's defaults — your answer for every project. */
    get profile() { return profile; },

    /**
     * Whether the check is actually on for the open project.
     *
     * Merged here and not on the backend because it is the one merged answer this side needs, and
     * it is one line: the project's own switch, or the profile's when the project inherits. Reading
     * only the project's switch is what made "Fix naming" grey on a project that had adopted the
     * profile's conventions and stated nothing of its own.
     */
    get enabled() {
      return project.config.enabled || (project.config.inherit && profile.config.enabled);
    },

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

    /** Point the project document at `root` and read it. A different root is a different document,
     *  so it is re-read rather than loaded — whatever was in the draft belonged to the old one. */
    async load(root: string) {
      if (loadedRoot === root) {
        await project.load();
        return;
      }
      loadedRoot = root;
      await project.reload();
    },

    /** The standard a pack's community uses, for the "Use the standard convention" button. */
    standardOf(packId: string): NamingRules {
      return catalog?.packs.find((p) => p.id === packId)?.standard ?? {};
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
  };
}

export const bennuNamingStore = createBennuNamingStore();
