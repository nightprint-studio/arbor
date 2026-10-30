/**
 * The profiler's state: what this machine can profile with, the recordings of the open project,
 * the one being looked at, and a recording in flight.
 *
 * A store (not panel state) because it is driven from outside the panel — the palette, the title
 * bar's Run configuration, `Ctrl+Shift+F5` — and because the editor reads its per-file numbers.
 *
 * ## Life of a recording
 *
 * `prepare` (backend creates a session and says what the process needs) → the program is started
 * with that plan (the Run console for Java, `bennu_profile_run_command` otherwise) → it exits, or
 * Stop is pressed → the backend parses on its own thread and answers with `profile-done`. The UI
 * never waits on the parse: `state` says `parsing` and the progress event counts events.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  EVT_PROFILE_DONE,
  EVT_PROFILE_PROGRESS,
  profileAttachStart,
  profileAttachStop,
  profileCompare,
  profileDelete,
  profileDetect,
  profileDumpRunning,
  profileFileMetrics,
  profileFinish,
  profileAllocByCode,
  profileFlame,
  profileGet,
  profileImport,
  profileList,
  profileListJvms,
  profileLocate,
  profilePrefsGet,
  profilePrefsSet,
  profilePrepare,
  profileRunCargo,
  profileRunCommand,
  type FileMetrics,
  type AllocByCode,
  type FlameKind,
  type FlameWire,
  type LocalJvm,
  type MethodDelta,
  type MethodStat,
  type ProfileDoneEvent,
  type ProfileLang,
  type ProfileMeta,
  type ProfileProgressEvent,
  type ProfilerDetect,
  type ProfilerFilter,
  type ProfilerPrefs,
  type ProfilerThresholds,
  type ProfileView,
  type StartPlan,
} from '$lib/ipc/bennu/profiler';
import { cancelRun, classIndex } from '$lib/ipc/bennu';
import {
  DEFAULT_PREFS,
  filterIsActive,
  keepClass,
  packageOf,
  type FilterContext,
} from '$lib/components/bennu/profiler/profiler-filter';
import { openLibraryClass } from '$lib/components/bennu/log-link';
import { projectStore } from './project.svelte';
import { bennuUiStore } from './ui.svelte';
import { bennuRunStore } from './run.svelte';
import { bennuRunConfigStore, cargoInvocationOf, type RunConfig } from './run-config.svelte';
import { profileTargetOf, type ProfileTarget } from '$lib/components/bennu/profiler/profiler-target';

export type ProfilerPhase = 'idle' | 'preparing' | 'recording' | 'parsing';
export type ProfilerTab = 'flame' | 'methods' | 'memory' | 'locks' | 'compare';

/** A recording being made. */
interface Live {
  sessionId: string;
  lang: ProfileLang;
  /** A JVM we launched: the main class (to find it for a dump) and the run to stop. */
  mainClass: string | null;
  runId: string | null;
  /** An attach: stopping runs `jcmd JFR.stop` instead of killing a run. */
  attached: boolean;
}

/** Quote one JVM argument for a run configuration's single-line `vmArgs` (split on spaces,
 *  single quotes group). The JFR option carries its own double quotes around a path with spaces. */
function vmArgToken(arg: string): string {
  return /\s/.test(arg) ? `'${arg}'` : arg;
}

function createProfilerStore() {
  let detect = $state<ProfilerDetect | null>(null);
  let detectError = $state<string | null>(null);
  let recordings = $state<ProfileMeta[]>([]);
  let activeId = $state<string | null>(null);
  let view = $state<ProfileView | null>(null);
  let flame = $state<FlameWire | null>(null);
  let thread = $state<string | null>(null);
  // What the flame graph weighs, and "who allocates" (loaded the first time the Memory tab asks for it).
  let flameKind = $state<FlameKind>('cpu');
  let allocByCode = $state<AllocByCode | null>(null);
  let samplingMs = $state<number | null>(null);
  let tab = $state<ProfilerTab>('flame');
  let phase = $state<ProfilerPhase>('idle');
  let progress = $state(0);
  let error = $state<string | null>(null);
  let live = $state<Live | null>(null);
  let jvms = $state<LocalJvm[]>([]);
  let compareWith = $state<string | null>(null);
  let deltas = $state<MethodDelta[]>([]);
  let loadingView = $state(false);

  // What the project remembers (`[profiler]` of its config), the search shared by every tab, and the
  // project's own classes — the one thing the "project only" filter needs from the index.
  let prefs = $state<ProfilerPrefs>(DEFAULT_PREFS);
  let query = $state('');
  let projectClasses = $state<ReadonlySet<string> | null>(null);
  let classesFor: string | null = null;

  // The "Profile…" dialog: opened from the title bar, the palette and the panel, so it is state here.
  let startOpen = $state(false);
  let startLang = $state<ProfileLang>('java');
  let detectFor: string | null = null;
  // The thresholds dialog, and a pending request to focus the search — both asked for from the palette and
  // the panel, and the search box may not exist yet when the request is made (the panel is still opening).
  let thresholdsOpen = $state(false);
  let searchFocusRequested = $state(false);

  // Per-file numbers for the editor, keyed by path. Invalidated whenever a recording lands.
  let fileMetrics = $state<Record<string, FileMetrics>>({});
  let metricsEpoch = $state(0);

  let unlisten: UnlistenFn[] = [];
  // The project whose recordings are in `recordings` — so the editor can ask for them without the
  // panel ever having been opened.
  let loadedFor: string | null = null;

  const root = () => projectStore.project?.root ?? null;

  async function attachEvents(): Promise<void> {
    if (unlisten.length) return;
    unlisten = [
      await listen<ProfileProgressEvent>(EVT_PROFILE_PROGRESS, (e) => {
        if (live && e.payload.session_id === live.sessionId) progress = e.payload.events;
      }),
      await listen<ProfileDoneEvent>(EVT_PROFILE_DONE, (e) => void onDone(e.payload)),
      await listen<{ run_id: string }>('arbor://bennu/run-exit', (e) => {
        // The profiled JVM ended by itself: it wrote the recording on exit, so read it.
        if (live && !live.attached && live.runId && e.payload.run_id === live.runId && phase === 'recording') {
          void beginParse(live);
        }
      }),
    ];
  }

  async function onDone(ev: ProfileDoneEvent): Promise<void> {
    if (!live || ev.session_id !== live.sessionId) return;
    live = null;
    phase = 'idle';
    progress = 0;
    if (!ev.ok) {
      error = ev.error ?? 'The recording could not be read.';
      toastStore.show(`Profiler: ${error}`, 'error');
      return;
    }
    error = null;
    await refresh();
    if (ev.meta) await select(ev.meta.id);
    bennuUiStore.showBottom('profiler');
  }

  async function beginParse(l: Live): Promise<void> {
    phase = 'parsing';
    try {
      await profileFinish(l.sessionId);
    } catch (e) {
      fail(e);
    }
  }

  function fail(e: unknown): void {
    phase = 'idle';
    live = null;
    error = e instanceof Error ? e.message : String(e);
    toastStore.show(`Profiler: ${error}`, 'error');
  }

  async function loadDetect(): Promise<void> {
    const r = root();
    if (!r) return;
    detectFor = r;
    try {
      detect = await profileDetect(r);
      detectError = null;
    } catch (e) {
      detect = null;
      detectError = e instanceof Error ? e.message : String(e);
    }
  }

  async function refresh(): Promise<void> {
    const r = root();
    loadedFor = r;
    if (!r) { recordings = []; return; }
    try {
      recordings = await profileList(r);
    } catch {
      recordings = [];
    }
    fileMetrics = {};
    metricsEpoch += 1;
    if (activeId && !recordings.some((m) => m.id === activeId)) {
      activeId = null; view = null; flame = null;
    }
    if (!activeId && recordings[0]) await select(recordings[0].id);
  }

  async function loadPrefs(): Promise<void> {
    const r = root();
    if (!r) { prefs = DEFAULT_PREFS; return; }
    // Another project's classes must not filter this one while its own are on their way.
    if (classesFor !== r) projectClasses = null;
    try {
      prefs = await profilePrefsGet(r);
    } catch {
      prefs = DEFAULT_PREFS;
    }
    if (prefs.filter.project_only) void ensureProjectClasses();
  }

  /** The project's classes, read once per project — only when the filter asks for them. */
  async function ensureProjectClasses(): Promise<void> {
    const r = root();
    if (!r || classesFor === r) return;
    classesFor = r;
    try {
      projectClasses = new Set((await classIndex(r)).map((c) => c.fqcn));
    } catch {
      classesFor = null;
      projectClasses = null;
    }
  }

  async function savePrefs(next: ProfilerPrefs): Promise<void> {
    const r = root();
    prefs = next;
    if (!r) return;
    try {
      await profilePrefsSet(r, next);
    } catch (e) {
      toastStore.show(`Profiler: could not save the settings (${e instanceof Error ? e.message : String(e)})`, 'error');
    }
  }

  function setFilter(filter: ProfilerFilter): void {
    if (filter.project_only) void ensureProjectClasses();
    void savePrefs({ ...prefs, filter });
  }

  function setThresholds(thresholds: ProfilerThresholds): void {
    void savePrefs({ ...prefs, thresholds });
  }

  async function select(id: string | null): Promise<void> {
    activeId = id;
    thread = null;
    flameKind = 'cpu';
    allocByCode = null;
    deltas = [];
    compareWith = null;
    const r = root();
    if (!id || !r) { view = null; flame = null; return; }
    loadingView = true;
    try {
      [view, flame] = await Promise.all([profileGet(r, id), profileFlame(r, id)]);
    } catch (e) {
      view = null; flame = null;
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loadingView = false;
    }
  }

  async function setThread(name: string | null): Promise<void> {
    const r = root();
    if (!r || !activeId) return;
    thread = name;
    try {
      flame = await profileFlame(r, activeId, name ?? undefined, flameKind);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Weigh the flame graph by CPU samples, by time blocked off the CPU, or by bytes allocated. */
  async function setFlameKind(kind: FlameKind): Promise<void> {
    const r = root();
    if (!r || !activeId) return;
    flameKind = kind;
    try {
      flame = await profileFlame(r, activeId, thread ?? undefined, kind);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Which of the project's methods allocated what — read once per recording, when the Memory tab first asks. */
  async function loadAllocByCode(): Promise<void> {
    const r = root();
    if (!r || !activeId || allocByCode) return;
    try {
      allocByCode = await profileAllocByCode(r, activeId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function compare(otherId: string | null): Promise<void> {
    const r = root();
    compareWith = otherId;
    if (!r || !activeId || !otherId) { deltas = []; return; }
    try {
      // `other` is the older one by convention: the table reads "what changed since then".
      deltas = await profileCompare(r, otherId, activeId);
    } catch (e) {
      deltas = [];
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function begin(lang: ProfileLang, label: string, extra: { pid?: number; samplyPath?: string; samplingMs?: number } = {}) {
    const r = root();
    if (!r) throw new Error('Open a project first.');
    if (phase !== 'idle') throw new Error('A recording is already in progress.');
    await attachEvents();
    phase = 'preparing';
    error = null;
    const { session_id, plan } = await profilePrepare({ root: r, lang, label, ...extra });
    return { r, session_id, plan };
  }

  /** Record a Java run configuration under JFR. */
  async function recordConfig(cfg: RunConfig): Promise<void> {
    try {
      const { r, session_id, plan } = await begin('java', cfg.name, { samplingMs: samplingMs ?? undefined });
      const withPlan = applyPlan(cfg, plan);
      live = { sessionId: session_id, lang: 'java', mainClass: cfg.mainClass, runId: null, attached: false };
      phase = 'recording';
      const before = bennuRunStore.tabs.length;
      await bennuRunStore.runConfig(r, withPlan);
      const tabs = bennuRunStore.tabs;
      const started = tabs.length > before ? tabs[tabs.length - 1] : null;
      if (started?.runId && live) {
        live = { ...live, runId: started.runId };
      } else if (live) {
        // The build failed or the launch was refused: nothing is recording.
        const l = live;
        live = null;
        phase = 'idle';
        void profileFinish(l.sessionId).catch(() => {});
      }
    } catch (e) {
      fail(e);
    }
  }

  function applyPlan(cfg: RunConfig, plan: StartPlan): RunConfig {
    const extra = plan.vm_args.map(vmArgToken).join(' ');
    return {
      ...cfg,
      vmArgs: [cfg.vmArgs.trim(), extra].filter(Boolean).join(' '),
      env: [...cfg.env, ...plan.env.map(([key, value]) => ({ key, value }))],
    };
  }

  /** Record a Cargo configuration: build the target, then run its binary under samply. */
  async function recordCargo(cfg: RunConfig): Promise<void> {
    try {
      const { r, session_id } = await begin('rust', cfg.name);
      live = { sessionId: session_id, lang: 'rust', mainClass: null, runId: null, attached: false };
      phase = 'recording';
      bennuUiStore.showBottom('run');
      const env = await bennuRunStore.launchEnv(r, cfg);
      if (!env) throw new Error('The environment file of this configuration could not be read.');
      const handle = await profileRunCargo(r, session_id, cargoInvocationOf(cfg), { workingDir: cfg.workingDir.trim(), env });
      live = { ...live, runId: handle.run_id };
    } catch (e) {
      fail(e);
      // The one failure with a place to fix it: no samply yet. Open the dialog on its tool.
      if (String(e instanceof Error ? e.message : e).includes('samply was not found')) openStart('rust');
    }
  }

  /** The selected run configuration, of whatever kind — the profiler button's subject. */
  function activeConfig(): RunConfig | null {
    const r = root();
    return r ? bennuRunConfigStore.activeFor(r) : null;
  }

  /** Whether the selected configuration can be profiled, and the tooltip that says why or what happens. */
  function profileTarget(): ProfileTarget {
    return profileTargetOf(activeConfig(), { available: detect ? detect.java.available : null, reason: detect?.java.reason ?? '' });
  }

  /** Profile the selected run configuration: a JVM application under JFR, a Cargo binary under samply. */
  async function recordActive(): Promise<void> {
    const cfg = activeConfig();
    const target = profileTarget();
    if (!cfg || !target.ok) {
      toastStore.show(target.note, 'info');
      return;
    }
    if (target.how === 'jfr') await recordConfig(cfg);
    else await recordCargo(cfg);
  }

  function openStart(lang?: ProfileLang): void {
    if (lang) startLang = lang;
    startOpen = true;
    void loadDetect();
    void listJvms();
  }

  /** Record a Node or native command (`argv[0]` is the program) in the Run console. */
  async function recordCommand(lang: 'node' | 'rust', argv: string[], label: string, samplyPath?: string): Promise<void> {
    try {
      if (argv.length === 0) throw new Error('Enter the command to profile.');
      const { r, session_id } = await begin(lang, label, { samplyPath });
      live = { sessionId: session_id, lang, mainClass: null, runId: null, attached: false };
      phase = 'recording';
      const handle = await profileRunCommand(r, session_id, argv);
      live = { ...live, runId: handle.run_id };
      bennuUiStore.showBottom('run');
    } catch (e) {
      fail(e);
    }
  }

  async function listJvms(): Promise<void> {
    const r = root();
    if (!r) return;
    try {
      jvms = await profileListJvms(r);
    } catch (e) {
      jvms = [];
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Start recording inside an already-running JVM. */
  async function attach(pid: number, label: string): Promise<void> {
    try {
      const { session_id } = await begin('java', label, { pid, samplingMs: samplingMs ?? undefined });
      live = { sessionId: session_id, lang: 'java', mainClass: null, runId: null, attached: true };
      await profileAttachStart(session_id);
      phase = 'recording';
    } catch (e) {
      fail(e);
    }
  }

  /** Stop the recording in progress and read it. */
  async function stop(): Promise<void> {
    const l = live;
    if (!l || phase !== 'recording') return;
    try {
      if (l.attached) {
        phase = 'parsing';
        await profileAttachStop(l.sessionId);
        return;
      }
      if (l.lang === 'java' && l.mainClass) {
        // Dump from the live JVM, then end the run: the file is otherwise only written on a normal exit.
        phase = 'parsing';
        await profileDumpRunning(l.sessionId, l.mainClass);
        if (l.runId) void cancelRun(l.runId);
        return;
      }
      // Node and native tools write their file when the process exits by itself.
      if (l.runId) {
        phase = 'parsing';
        await cancelRun(l.runId);
        await profileFinish(l.sessionId);
      }
    } catch (e) {
      fail(e);
    }
  }

  async function importFile(path: string): Promise<void> {
    const r = root();
    if (!r) return;
    try {
      await attachEvents();
      phase = 'parsing';
      error = null;
      const sessionId = await profileImport(r, path);
      live = { sessionId, lang: 'java', mainClass: null, runId: null, attached: false };
    } catch (e) {
      fail(e);
    }
  }

  async function remove(id: string): Promise<void> {
    const r = root();
    if (!r) return;
    await profileDelete(r, id);
    await refresh();
  }

  /** Open the source of a method — a project class, a library class, or a script file. */
  async function goTo(m: Pick<MethodStat, 'class' | 'method' | 'arity' | 'file' | 'decl_line'>): Promise<void> {
    const r = root();
    if (!r) return;
    try {
      if (m.file) {
        await projectStore.openFile(m.file.replace(/\\/g, '/'));
        if (m.decl_line) bennuUiStore.requestGoto(m.decl_line);
        return;
      }
      const at = await profileLocate(r, m.class, m.method, m.arity);
      if (at) {
        await projectStore.openFile(at.file.replace(/\\/g, '/'));
        if (at.line) bennuUiStore.requestGoto(at.line);
        return;
      }
      await openLibraryClass(m.class.replace(/\$.*$/, ''), m.method);
    } catch {
      toastStore.show('That method has no source here (JDK internals or generated code).', 'info');
    }
  }

  /** Numbers for the editor's hints. Cheap when nothing is recorded: one list, cached per epoch. */
  async function metricsFor(file: string, source: string): Promise<FileMetrics | null> {
    const r = root();
    if (!r || recordings.length === 0) return null;
    try {
      const fm = await profileFileMetrics(r, file, source);
      fileMetrics = { ...fileMetrics, [file]: fm };
      return fm;
    } catch {
      return null;
    }
  }

  return {
    get detect() { return detect; },
    get detectError() { return detectError; },
    get recordings() { return recordings; },
    get activeId() { return activeId; },
    get view() { return view; },
    get flame() { return flame; },
    get thread() { return thread; },
    get flameKind() { return flameKind; },
    setFlameKind,
    get allocByCode() { return allocByCode; },
    loadAllocByCode,
    /** The execution sampler's interval for the next Java recording, in ms; `null` keeps JFR's own 10 ms. */
    get samplingMs() { return samplingMs; },
    setSamplingMs(ms: number | null) { samplingMs = ms; },
    get tab() { return tab; },
    setTab(t: ProfilerTab) { tab = t; },
    get phase() { return phase; },
    get progress() { return progress; },
    get error() { return error; },
    get live() { return live; },
    get jvms() { return jvms; },
    get compareWith() { return compareWith; },
    get deltas() { return deltas; },
    get loadingView() { return loadingView; },
    get thresholdsOpen() { return thresholdsOpen; },
    openThresholds() { thresholdsOpen = true; },
    closeThresholds() { thresholdsOpen = false; },
    get searchFocusRequested() { return searchFocusRequested; },
    /** Ask the visible tab's search box for focus; it takes the request when it exists. */
    requestSearchFocus() { searchFocusRequested = true; },
    consumeSearchFocus() { searchFocusRequested = false; },
    get startOpen() { return startOpen; },
    get startLang() { return startLang; },
    setStartLang(l: ProfileLang) { startLang = l; },
    openStart,
    closeStart() { startOpen = false; },
    profileTarget,
    recordActive,
    /** Check what this machine can profile with, once per project — what the title bar's button needs to know. */
    ensureDetect(): void {
      if (detectFor !== root()) void loadDetect();
    },
    get prefs() { return prefs; },
    get filter() { return prefs.filter; },
    get thresholds() { return prefs.thresholds; },
    get filterActive() { return filterIsActive(prefs.filter); },
    get query() { return query; },
    setQuery(q: string) { query = q; },
    /** The predicate every tab applies; reads the filter and the project's classes, so a `$derived` over it re-runs when either changes. */
    keep(cls: string): boolean {
      const ctx: FilterContext = { filter: prefs.filter, projectClasses };
      return keepClass(ctx, cls);
    },
    /** The packages of the project's own classes, for suggesting what to filter on. Empty until they are loaded. */
    get projectPackages(): string[] {
      return projectClasses ? [...new Set([...projectClasses].map(packageOf).filter(Boolean))].sort() : [];
    },
    ensureProjectClasses,
    loadPrefs,
    setFilter,
    setThresholds,
    /** Bumped whenever recordings change — what the editor's hint effect depends on. */
    get metricsEpoch() { return metricsEpoch; },
    get hasRecordings() { return recordings.length > 0; },
    get activeMeta() { return recordings.find((m) => m.id === activeId) ?? null; },
    fileMetricsFor(file: string): FileMetrics | null { return fileMetrics[file] ?? null; },
    /** The run configuration the title bar has selected, when it is a JVM application. */
    activeJavaConfig(): RunConfig | null {
      const r = root();
      const cfg = r ? bennuRunConfigStore.activeFor(r) : null;
      return cfg && (cfg.kind === 'application' || cfg.kind === 'springboot') ? cfg : null;
    },
    loadDetect,
    refresh,
    /** Load the recordings of the open project once (no-op when already loaded for it). */
    ensureLoaded(): void {
      if (loadedFor !== root()) { void refresh(); void loadPrefs(); }
    },
    select,
    setThread,
    compare,
    recordConfig,
    recordCommand,
    listJvms,
    attach,
    stop,
    importFile,
    remove,
    goTo,
    metricsFor,
    /** Open the tool window and make sure it has something to show. */
    async open(): Promise<void> {
      bennuUiStore.showBottom('profiler');
      await Promise.all([loadDetect(), refresh()]);
    },
  };
}

export const bennuProfilerStore = createProfilerStore();
