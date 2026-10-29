/**
 * New Project wizard store — the catalog, the chosen generator and its form, the live preview, and
 * the creation run.
 *
 * Whether the modal is open lives in `bennuUiStore` (like every other Bennu overlay); everything
 * the wizard *does* lives here. The remembered choices (last location, last generator, Git toggle)
 * are persisted through Bennu's own config (`get/set_bennu_new_project`), never `localStorage`.
 *
 * Two guards against stale answers: every async request carries a sequence number and a late reply
 * is dropped, because the form is rebuilt while the user types — a preview for "dem" must not
 * overwrite the one for "demo".
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  cancelProject, createProject, getNewProjectPrefs, newProjectCatalog, newProjectFields,
  onProjectRun, previewProject, setNewProjectPrefs,
  type FieldValue, type GeneratorField, type NewProjectCatalog, type NewProjectGenerator,
  type NewProjectPrefs, type ProjectPreview, type ProjectRunEvent,
} from '$lib/ipc/bennu/new-project';
import {
  buildRequest, defaultsOf, initialGenerator, mergeValues, newRunId, type FormValues,
} from '$lib/components/bennu/new-project/new-project-helpers';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

export type Phase = 'form' | 'running' | 'error';
export type OpenMode = 'add' | 'replace';

const MAX_LOG_LINES = 2000;
const PREVIEW_DEBOUNCE_MS = 180;

function createNewProjectStore() {
  let catalog = $state<NewProjectCatalog | null>(null);
  let catalogError = $state<string | null>(null);
  let prefs = $state<NewProjectPrefs>({ last_location: '', last_generator: '', git_init: null, package_manager: '', last_group: '' });

  let generatorId = $state('');
  let name = $state('');
  let location = $state('');
  let git = $state(true);
  let openMode = $state<OpenMode>('add');

  let fields = $state<GeneratorField[]>([]);
  let values = $state<FormValues>({});
  let fieldsLoading = $state(false);
  let fieldsError = $state<string | null>(null);
  let fieldsStale = $state(false);

  let preview = $state<ProjectPreview | null>(null);
  let previewing = $state(false);

  /** Create was asked for while the form wasn't valid — the form then shows every error at once. */
  let attempted = $state(false);

  let phase = $state<Phase>('form');
  let stage = $state('');
  let log = $state<string[]>([]);
  let runError = $state<string | null>(null);
  let runId = '';
  let unlisten: UnlistenFn | null = null;

  let fieldsSeq = 0;
  let previewSeq = 0;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  const generator = $derived<NewProjectGenerator | null>(catalog?.generators.find((g) => g.id === generatorId) ?? null);

  // ── loading ────────────────────────────────────────────────────────────────────────────

  async function loadFields(refresh = false): Promise<void> {
    const mine = ++fieldsSeq;
    const g = catalog?.generators.find((x) => x.id === generatorId);
    if (!g) return;
    fieldsLoading = true;
    fieldsError = null;
    try {
      const res = await newProjectFields(g.id, refresh);
      if (mine !== fieldsSeq) return;
      fields = res.fields;
      fieldsStale = res.stale;
      values = mergeValues(res.fields, values);
      applyRememberedChoices();
      schedulePreview(0);
    } catch (e) {
      if (mine !== fieldsSeq) return;
      fieldsError = String(e);
    } finally {
      if (mine === fieldsSeq) fieldsLoading = false;
    }
  }

  /** The last group / package manager used, where the form has such a field and the user hasn't typed. */
  function applyRememberedChoices(): void {
    const has = (k: string) => fields.some((f) => f.key === k);
    const def = (k: string) => fields.find((f) => f.key === k)?.default;
    if (prefs.last_group && has('group') && values.group === def('group')) values = { ...values, group: prefs.last_group };
    if (prefs.package_manager && has('pm')) {
      const ok = fields.find((f) => f.key === 'pm')?.options.some((o) => o.value === prefs.package_manager);
      if (ok && values.pm === def('pm')) values = { ...values, pm: prefs.package_manager };
    }
  }

  function schedulePreview(delay = PREVIEW_DEBOUNCE_MS): void {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(() => void runPreview(), delay);
  }

  async function runPreview(): Promise<void> {
    if (!generatorId || fieldsLoading) return;
    const mine = ++previewSeq;
    previewing = true;
    try {
      const res = await previewProject(request());
      if (mine === previewSeq) preview = res;
    } catch (e) {
      if (mine === previewSeq) preview = null;
      console.warn('[new-project] preview failed', e);
    } finally {
      if (mine === previewSeq) previewing = false;
    }
  }

  function request() {
    return buildRequest(generatorId, name, location, git, fields, values);
  }

  // ── the run ────────────────────────────────────────────────────────────────────────────

  function pushLog(line: string): void {
    log = log.length >= MAX_LOG_LINES ? [...log.slice(-MAX_LOG_LINES + 1), line] : [...log, line];
  }

  async function detach(): Promise<void> {
    const off = unlisten;
    unlisten = null;
    off?.();
  }

  async function onRunEvent(e: ProjectRunEvent): Promise<void> {
    switch (e.kind) {
      case 'stage':
        stage = e.text;
        pushLog(`── ${e.text}`);
        break;
      case 'log':
        pushLog(e.text);
        break;
      case 'cancelled':
        await detach();
        phase = 'form';
        toastStore.show('Project creation cancelled — nothing was left behind', 'info');
        break;
      case 'error':
        await detach();
        runError = e.message;
        phase = 'error';
        break;
      case 'done':
        await detach();
        await finish(e.path, e.open, e.warnings);
        break;
    }
  }

  async function finish(path: string, open: string | null, warnings: string[]): Promise<void> {
    void remember();
    try {
      if (openMode === 'add' && projectStore.project) {
        if (!(await projectStore.addProject(path))) await projectStore.openProject(path);
      } else {
        await projectStore.openProject(path);
      }
      if (open) await projectStore.openFile(open);
    } catch (err) {
      toastStore.show(`The project was created, but could not be opened: ${err}`, 'warning');
    }
    for (const w of warnings) toastStore.show(w, 'warning');
    phase = 'form';
    toastStore.show(`Created ${name.trim()}`, 'success');
    bennuUiStore.closeNewProject();
  }

  /** Persist what the next run should start from. */
  async function remember(): Promise<void> {
    const next: NewProjectPrefs = {
      last_location: location.trim(),
      last_generator: generatorId,
      git_init: git,
      package_manager: typeof values.pm === 'string' && values.pm ? values.pm : prefs.package_manager,
      last_group: typeof values.group === 'string' && values.group ? values.group : prefs.last_group,
    };
    prefs = next;
    await setNewProjectPrefs(next).catch(() => {});
  }

  // ── public ─────────────────────────────────────────────────────────────────────────────

  return {
    get catalog() { return catalog; },
    get catalogError() { return catalogError; },
    get generatorId() { return generatorId; },
    get generator() { return generator; },
    get name() { return name; },
    get location() { return location; },
    get git() { return git; },
    get openMode() { return openMode; },
    get fields() { return fields; },
    get values() { return values; },
    get fieldsLoading() { return fieldsLoading; },
    get fieldsError() { return fieldsError; },
    get fieldsStale() { return fieldsStale; },
    get preview() { return preview; },
    get previewing() { return previewing; },
    get phase() { return phase; },
    get stage() { return stage; },
    get log() { return log; },
    get runError() { return runError; },
    get busy() { return phase === 'running'; },
    get attempted() { return attempted; },
    /** Whether Create would be accepted right now. */
    get canCreate() { return phase === 'form' && !fieldsLoading && !fieldsError && !!preview?.ok && !previewing; },

    /** Load the catalog and the remembered choices, then select a generator. Called when the modal opens. */
    async init(): Promise<void> {
      phase = 'form';
      attempted = false;
      runError = null;
      log = [];
      preview = null;
      catalogError = null;
      try {
        const [cat, saved] = await Promise.all([
          newProjectCatalog(),
          getNewProjectPrefs().catch(() => prefs),
        ]);
        catalog = cat;
        prefs = saved;
        git = saved.git_init ?? true;
        if (!location) {
          location = saved.last_location || (cat.home ? `${cat.home}/BennuProjects` : '');
        }
        openMode = projectStore.project ? 'add' : 'replace';
        const first = initialGenerator(cat.generators, saved.last_generator);
        if (first) await this.selectGenerator(first.id);
      } catch (e) {
        catalogError = String(e);
      }
    },

    async selectGenerator(id: string): Promise<void> {
      if (phase === 'running' || id === generatorId) return;
      const g = catalog?.generators.find((x) => x.id === id);
      if (!g) return;
      generatorId = id;
      preview = null;
      // Show the form's shape at once; remote options replace it when they arrive.
      fields = [];
      values = {};
      await loadFields();
    },

    retryFields(): Promise<void> {
      return loadFields(true);
    },

    setName(v: string) { name = v; schedulePreview(); },
    setLocation(v: string) { location = v; schedulePreview(); },
    setGit(v: boolean) { git = v; schedulePreview(); },
    setOpenMode(v: OpenMode) { openMode = v; },
    setValue(key: string, v: FieldValue) {
      values = { ...values, [key]: v };
      schedulePreview();
    },
    resetValues() {
      values = defaultsOf(fields);
      schedulePreview(0);
    },

    /** Start creating. The run narrates itself through events; the store opens the project when it ends. */
    async create(): Promise<void> {
      if (phase !== 'form') return;
      if (!this.canCreate) {
        attempted = true;
        return;
      }
      runId = newRunId();
      phase = 'running';
      stage = 'Starting';
      log = [];
      runError = null;
      try {
        unlisten = await onProjectRun(runId, (e) => void onRunEvent(e));
        await createProject(runId, request());
      } catch (e) {
        await detach();
        runError = String(e);
        phase = 'error';
      }
    },

    async cancel(): Promise<void> {
      if (phase !== 'running') return;
      stage = 'Cancelling';
      await cancelProject(runId).catch(() => false);
    },

    /** Back from an error to the form, keeping everything typed. */
    backToForm() {
      phase = 'form';
      runError = null;
      schedulePreview(0);
    },

    /** Called when the modal closes. A run in progress is cancelled, not abandoned. */
    async dispose(): Promise<void> {
      if (previewTimer) clearTimeout(previewTimer);
      fieldsSeq++;
      previewSeq++;
      if (phase === 'running') await cancelProject(runId).catch(() => false);
      await detach();
      phase = 'form';
    },
  };
}

export const newProjectStore = createNewProjectStore();
