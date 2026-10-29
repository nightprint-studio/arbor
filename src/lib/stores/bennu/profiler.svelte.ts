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
  profileFlame,
  profileGet,
  profileImport,
  profileList,
  profileListJvms,
  profileLocate,
  profilePrepare,
  profileRunCommand,
  type FileMetrics,
  type FlameWire,
  type LocalJvm,
  type MethodDelta,
  type MethodStat,
  type ProfileDoneEvent,
  type ProfileLang,
  type ProfileMeta,
  type ProfileProgressEvent,
  type ProfilerDetect,
  type ProfileView,
  type StartPlan,
} from '$lib/ipc/bennu/profiler';
import { cancelRun } from '$lib/ipc/bennu';
import { openLibraryClass } from '$lib/components/bennu/log-link';
import { projectStore } from './project.svelte';
import { bennuUiStore } from './ui.svelte';
import { bennuRunStore } from './run.svelte';
import { bennuRunConfigStore, type RunConfig } from './run-config.svelte';

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
  let tab = $state<ProfilerTab>('flame');
  let phase = $state<ProfilerPhase>('idle');
  let progress = $state(0);
  let error = $state<string | null>(null);
  let live = $state<Live | null>(null);
  let jvms = $state<LocalJvm[]>([]);
  let compareWith = $state<string | null>(null);
  let deltas = $state<MethodDelta[]>([]);
  let loadingView = $state(false);

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

  async function select(id: string | null): Promise<void> {
    activeId = id;
    thread = null;
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
      flame = await profileFlame(r, activeId, name ?? undefined);
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

  async function begin(lang: ProfileLang, label: string, extra: { pid?: number; samplyPath?: string } = {}) {
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
      const { r, session_id, plan } = await begin('java', cfg.name);
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
      const { session_id } = await begin('java', label, { pid });
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
      if (loadedFor !== root()) void refresh();
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
