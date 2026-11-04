/**
 * The Java formatter's style, in all its layers — what Settings › Java › Formatter edits and what
 * format-on-save reads.
 *
 * Rune store (CLAUDE.md · "Store pattern"). Persistence is the backend's: the global style lives in
 * the app config, the project's sparse override in `<repo>/.arbor/bennu/config.toml [format]`. Nothing
 * here touches `localStorage`.
 *
 * ## One editing surface, two scopes
 *
 * * **Global** edits the full global style. Rows show the global value; a small badge marks a key
 *   that this project's `.editorconfig`, detection or own override replaces.
 * * **This project** edits the project's *sparse* override. Rows show the effective value; setting a
 *   key to what the layers below already say removes it from the override, so the file only ever
 *   names what the project really differs on.
 *
 * The merge is repeated locally (`format-layers.ts`) so an edit shows at once; every write goes to
 * the backend debounced and in order, so the last edit is the last write.
 */
import {
  formatDetect, formatPreview, formatSetGlobal, formatSetProject, formatSettings,
} from '$lib/ipc/bennu/format-style';
import {
  DEFAULT_FORMAT_STYLE, effectiveStyle, originOf, sameValue, styleBelowProject, type StyleLayers,
} from '$lib/components/bennu/format-layers';
import { FORMAT_SAMPLE } from '$lib/components/bennu/format-sample';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import type {
  FormatDetection, FormatKey, FormatLayer, FormatPatch, FormatStyle,
} from '$lib/types/bennu-format';

export type FormatScope = 'global' | 'project';

/** Edits are written this long after the last one — a stepper held down is one write, not twenty. */
const PERSIST_DEBOUNCE_MS = 300;
/** The preview follows the edit this closely. */
const PREVIEW_DEBOUNCE_MS = 150;

function createFormatStyleStore() {
  let root = $state<string | null>(null);
  let scope = $state<FormatScope>('global');
  let ready = $state(false);
  let defaults = $state<FormatStyle>(DEFAULT_FORMAT_STYLE);
  let global = $state<FormatStyle>(DEFAULT_FORMAT_STYLE);
  let project = $state<FormatPatch | null>(null);
  let editorconfig = $state<FormatPatch | null>(null);
  let detected = $state<FormatPatch | null>(null);

  let detection = $state<FormatDetection | null>(null);
  let detecting = $state(false);

  let previewText = $state('');
  let previewError = $state<string | null>(null);

  const layers = $derived<StyleLayers>({ defaults, global, detected, editorconfig, project });
  const effective = $derived(effectiveStyle(layers));
  /** The scope in force: a project scope with no project open is the global one. */
  const activeScope = $derived<FormatScope>(root ? scope : 'global');
  /** What the rows show, and what the preview formats. */
  const viewStyle = $derived(activeScope === 'project' ? effective : global);

  // ── Loading ────────────────────────────────────────────────────────────────────────────────
  let loadToken = 0;

  /** Read every layer for the project at `projectRoot` (the global ones alone without one). */
  async function load(projectRoot?: string | null): Promise<void> {
    const mine = ++loadToken;
    const nextRoot = projectRoot || null;
    try {
      const s = await formatSettings(nextRoot ?? undefined);
      if (mine !== loadToken) return; // a newer load (another project) supersedes this one
      // A measurement belongs to its project; re-reading the same one (the page opening) keeps it.
      if (nextRoot !== root) detection = null;
      root = nextRoot;
      defaults = s.defaults;
      global = s.global;
      project = s.project && Object.keys(s.project).length ? s.project : null;
      editorconfig = s.editorconfig;
      detected = s.detected;
      ready = true;
      schedulePreview();
    } catch {
      // The backend is not up yet (it announces itself and the window loads again) or the
      // command is missing: keep the defaults, and let the caller's next load fix it.
      if (mine === loadToken) {
        // Another project's layers must not linger under this root.
        if (nextRoot !== root) { project = null; editorconfig = null; detected = null; detection = null; }
        root = nextRoot;
      }
    }
  }

  // ── Persistence ────────────────────────────────────────────────────────────────────────────
  let writeChain: Promise<void> = Promise.resolve();
  let globalTimer: ReturnType<typeof setTimeout> | undefined;
  let projectTimer: ReturnType<typeof setTimeout> | undefined;

  function enqueue(write: () => Promise<void>) {
    writeChain = writeChain.then(write).catch((err) => {
      toastStore.show(`Could not save the formatter style: ${err}`, 'error');
    });
  }

  function persistGlobal() {
    clearTimeout(globalTimer);
    globalTimer = setTimeout(() => {
      globalTimer = undefined;
      const style = global;
      enqueue(() => formatSetGlobal(style));
    }, PERSIST_DEBOUNCE_MS);
  }

  function persistProject() {
    const target = root;
    if (!target) return;
    clearTimeout(projectTimer);
    projectTimer = setTimeout(() => {
      projectTimer = undefined;
      const overrides = project;
      enqueue(() => formatSetProject(target, overrides));
    }, PERSIST_DEBOUNCE_MS);
  }

  /** Write anything still waiting out its debounce — for when the page goes away. */
  function flush() {
    if (globalTimer !== undefined) {
      clearTimeout(globalTimer);
      globalTimer = undefined;
      const style = global;
      enqueue(() => formatSetGlobal(style));
    }
    if (projectTimer !== undefined && root) {
      clearTimeout(projectTimer);
      projectTimer = undefined;
      const target = root;
      const overrides = project;
      enqueue(() => formatSetProject(target, overrides));
    }
  }

  // ── Preview ────────────────────────────────────────────────────────────────────────────────
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let previewToken = 0;

  function schedulePreview() {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(runPreview, PREVIEW_DEBOUNCE_MS);
  }

  async function runPreview() {
    const mine = ++previewToken;
    try {
      const text = await formatPreview(viewStyle, FORMAT_SAMPLE);
      if (mine !== previewToken) return; // a newer edit already asked; this answer is stale
      previewText = text;
      previewError = null;
    } catch (err) {
      if (mine !== previewToken) return;
      previewError = String(err);
    }
  }

  // ── Editing ────────────────────────────────────────────────────────────────────────────────
  function set<K extends FormatKey>(key: K, value: FormatStyle[K]) {
    if (activeScope === 'global') {
      global = { ...global, [key]: value } as FormatStyle;
      persistGlobal();
    } else {
      const next: FormatPatch = { ...(project ?? {}) };
      // Equal to what the layers below say: not an override, so it leaves the file.
      if (sameValue(styleBelowProject(layers)[key], value)) delete next[key];
      else (next as Record<string, unknown>)[key] = value;
      project = Object.keys(next).length ? next : null;
      persistProject();
    }
    schedulePreview();
  }

  /** Whether `key` has something of its own in the scope being edited — what a reset would undo. */
  function isModified(key: FormatKey): boolean {
    return activeScope === 'project'
      ? !!project && key in project
      : !sameValue(global[key], defaults[key]);
  }

  function resetKey(key: FormatKey) {
    if (activeScope === 'global') set(key, defaults[key]);
    else if (project && key in project) {
      const next = { ...project };
      delete next[key];
      project = Object.keys(next).length ? next : null;
      persistProject();
      schedulePreview();
    }
  }

  /** Global: back to the defaults. Project: drop the whole override. */
  function resetAll() {
    if (activeScope === 'global') {
      global = defaults;
      persistGlobal();
    } else {
      project = null;
      persistProject();
    }
    schedulePreview();
  }

  /** The layer a key's *effective* value comes from; `global` and `default` are not worth a badge. */
  function layerOf(key: FormatKey): FormatLayer {
    return originOf(key, layers);
  }

  // ── Detection ──────────────────────────────────────────────────────────────────────────────
  async function detect(): Promise<void> {
    const target = root;
    if (!target || detecting) return;
    detecting = true;
    try {
      const result = await formatDetect(target);
      if (root === target) detection = result;
    } catch (err) {
      toastStore.show(`Could not measure the project: ${err}`, 'error');
    } finally {
      detecting = false;
    }
  }

  /** Store what detection found as the project's override, and show the project's view of it. */
  function applyDetection() {
    if (!detection || !root) return;
    project = { ...(project ?? {}), ...detection.patch };
    if (!Object.keys(project).length) project = null;
    detection = null;
    scope = 'project';
    persistProject();
    schedulePreview();
  }

  return {
    get root() { return root; },
    get ready() { return ready; },
    get scope() { return activeScope; },
    get hasProject() { return !!root; },
    /** The style the rows show and the preview formats. */
    get style() { return viewStyle; },
    /** The style that formats a file in the open project. */
    get effective() { return effective; },
    get defaults() { return defaults; },
    get hasProjectOverride() { return !!project; },
    get editorconfigFound() { return !!editorconfig; },
    get detection() { return detection; },
    get detecting() { return detecting; },
    get previewText() { return previewText; },
    get previewError() { return previewError; },
    /** Whether a Java buffer is formatted on save, in the project open. */
    get formatOnSave() { return effective.format_on_save; },
    load,
    flush,
    setScope(next: FormatScope) {
      scope = next;
      schedulePreview();
    },
    set,
    isModified,
    resetKey,
    resetAll,
    layerOf,
    detect,
    applyDetection,
    dismissDetection() { detection = null; },
  };
}

export const bennuFormatStyleStore = createFormatStyleStore();
