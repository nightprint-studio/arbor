/**
 * Bennu run/build store — drives the titlebar Run/Build controls, the Build tool
 * window and the Run console.
 *
 * The backend (`bennu-be`, `build.rs`) does the real work:
 *   • `bennu_build`  → `mvn -q -o compile` (offline, project JDK) with a `javac`
 *     fallback; the raw log streams as `arbor://bennu/build-output`, the resolved
 *     promise carries the parsed {@link BuildResult} (tool · ok · diagnostics). A
 *     clean build re-indexes `target/classes` on the BE.
 *   • `bennu_run`    → `java <vm…> -cp target/classes:deps <mainClass> <args…>`;
 *     stdout/stderr stream as `arbor://bennu/run-output`, ending with
 *     `arbor://bennu/run-exit`.
 *   • `bennu_run_input`  → a line to the program's stdin.
 *   • `bennu_cancel_run` → kill a live run's process tree.
 *
 * ## The build log, and one tab per run
 *
 * A compile log and a program's output are different things that happen to arrive
 * the same way, and they used to share one array: running an app appended the JVM's
 * output under Maven's, and the next build wiped both. So the build log ({@link lines})
 * is its own, and every launch gets a {@link RunTab} — its transcript plus what it was,
 * when it started and how it ended.
 *
 * Tabs rather than one buffer because a run is something that HAPPENED: comparing this
 * run against the previous one is most of what a console is for, and a single buffer
 * threw the previous one away the moment you pressed ▷ again.
 *
 * ## Several at once
 *
 * Any number of programs can be running, each in its own tab: a server and the client that
 * talks to it, or the same project's two entry points side by side. The backend was always
 * built for it — every launch gets its own process and its own run id, and the output and exit
 * events carry that id — so what the tabs show is the shape the backend already had.
 *
 * Two things follow. Stop and stdin act on the tab **in front**, not on "the live one", since
 * there may be several (see `targetTab`); and a compile is still exclusive, because
 * `bennu_build` takes the backend's build guard, so a launch is refused only while one is
 * compiling.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  build as ipcBuild, validateProject as ipcValidateProject, run as ipcRun,
  cancelRun as ipcCancelRun, cancelValidation as ipcCancelValidation,
  runInput as ipcRunInput,
  runScript as ipcRunScript,
} from '$lib/ipc/bennu';
import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';
import type {
  BuildResult, BuildDiagnostic, ProjectValidationResult,
} from '$lib/types/bennu';
import type { LogLevel, LogPiece } from '$lib/types/log';
import { bennuUiStore } from './ui.svelte';
import { bennuDiagnosticsStore } from './diagnostics.svelte';
// One-way edge (the debugger knows nothing about console tabs): a run and its debug session share
// an id, so the console is what says which session the Debug panel is looking at.
import { bennuDebugStore } from './debug.svelte';
import {
  bennuRunConfigStore, splitArgs, envRecord, isRunKind, cargoInvocationOf, type RunConfig,
} from './run-config.svelte';
import {
  cargoDebug as ipcCargoDebug, cargoRun as ipcCargoRun, cargoWorkspace, type CargoInvocation,
} from '$lib/ipc/bennu/cargo';
// A `junit` configuration is launched by the TEST runner, not by this one — see `runConfig`.
// (`tests` imports only a TYPE from here, so the edge is erased at build and there is no
// runtime cycle.)
import { bennuTestStore } from './tests.svelte';
// The project's entry points, read once and shared with the run-config editor — this path
// used to scan the sources on every press of ▷.
import { bennuMainClassStore } from './main-classes.svelte';
import type { TestScope } from '$lib/types/bennu';
// One-way edge (the project store knows nothing about runs): `runPreferred` needs the
// active project's KIND, because "Validate (no compile)" is a Java-only build.
import { projectStore } from './project.svelte';

/** The two build kinds the split-button offers. */
export type BuildType = 'mvn' | 'validate';

/** One streamed log line + which channel it came from (drives colouring). */
export interface RunLogLine {
  text: string;
  /** `out` = stdout / mvn log · `err` = stderr · `meta` = our own status lines ·
   *  `in` = a line YOU typed, echoed back the way a terminal echoes it. */
  stream: 'out' | 'err' | 'meta' | 'in';
  /** The interpreted severity (`arbor-logscan`, backend), when the line said so or
   *  inherited it from the one above. Absent on lines this store wrote itself. */
  level?: LogLevel | null;
  /** The line already cut into what its parts ARE — levels, timestamps, paths, stack
   *  frames. Absent means "render the text": a line nobody interpreted still shows. */
  pieces?: LogPiece[];
}

/** What the backend adds to an output event beyond the text — see {@link RunLogLine}. */
interface LogAnnotation {
  level?: LogLevel | null;
  pieces?: LogPiece[];
}

/**
 * Cap the retained log so a chatty build/run can't grow the buffer unbounded.
 *
 * It used to be 3000, which was really a cap on the DOM: every retained line was a rendered
 * row, and a Tomcat or Spring Boot startup reached it in seconds — so the beginning of the
 * run, which is where the interesting failures are, had already scrolled out of existence by
 * the time you looked. The console renders only what is on screen now
 * ({@link BennuConsole}), so what this bounds is memory, and memory affords a great deal more.
 */
const MAX_LINES = 10_000;

/** Render a millisecond duration compactly (`340ms` / `1.2s` / `1m 05s`). */
export function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${String(Math.round(s % 60)).padStart(2, '0')}s`;
}

/**
 * What a launch consists of — kept so Rerun can repeat it exactly.
 *
 * A union because the two launchers have almost nothing in common: a JVM run needs a classpath
 * built for it and is preceded by a compile, while a cargo command *is* the build and takes a
 * command line. Keeping one flat shape with half its fields unused would mean every reader had to
 * know which half applied, and `rerun` would have to guess.
 */
/** A shell script launch — `.sh`, `.cmd`/`.bat`, `.ps1`. */
interface ScriptRunSpec {
  kind: 'script';
  root: string;
  /** Absolute path of the script. The backend picks the interpreter from its extension. */
  file: string;
  args: string[];
  workingDir: string;
  env: Record<string, string>;
  label: string;
}

type RunSpec = JvmRunSpec | CargoRunSpec | ScriptRunSpec;

/** A `java` launch. */
interface JvmRunSpec {
  kind: 'jvm';
  root: string;
  /** The Maven module, relative to the root. Empty = the root module. */
  module: string;
  mainClass: string;
  args: string[];
  vmArgs: string[];
  workingDir: string;
  env: Record<string, string>;
  /** The run configuration's name, for the console header. Empty for an ad-hoc launch. */
  label: string;
  /** Launch under the debugger. Part of the spec, so a tab's ⟳ repeats the run it *was* —
   *  re-running a debug session as a plain run would be a different thing wearing the same
   *  label. */
  debug: boolean;
  /** Hold the VM before `main`. The configuration's choice; see `RunConfig.debugSuspend`. */
  debugSuspend: boolean;
  /** Which Maven scopes the run classpath is resolved at. Part of the spec so a tab's ⟳
   *  repeats the classpath it *ran with*; see `RunConfig.classpathScope`. */
  classpathScope: string;
}

/** A `cargo <subcommand>` launch. */
interface CargoRunSpec {
  kind: 'cargo';
  root: string;
  /** The whole command line, as the backend's `Invocation`. Held verbatim so ⟳ repeats the flags
   *  the run actually had rather than re-deriving them from a configuration that may have been
   *  edited since. */
  invocation: CargoInvocation;
  workingDir: string;
  env: Record<string, string>;
  /** The console tab's label. */
  label: string;
  /**
   * Whether to launch it under a debugger.
   *
   * Not a separate kind of spec, because it is not a separate configuration: debugging is the run you
   * already set up with a debugger attached. What differs is only which verb the backend is asked for,
   * and that the build happens before the process exists — see `cargoDebug`.
   */
  debug?: boolean;
}

/**
 * One run, with its own transcript — what the console shows as a tab.
 *
 * A run is a thing that happened, not a slot that gets overwritten: comparing this run's
 * output against the previous one is most of what a console is for, and the version that
 * kept a single buffer threw the previous one away the moment you pressed ▷ again.
 */
export interface RunTab {
  /** Frontend id, stable for the tab's life. Not the backend's run id. */
  id: string;
  /** The run configuration's name — the tab's label. */
  label: string;
  /** What was launched: a main class, or the cargo command. Shown when the backend has not yet
   *  answered with the real command line. */
  subject: string;
  /** The command the backend actually spawned; empty until it answers. */
  command: string;
  workingDir: string;
  lines: RunLogLine[];
  /** The backend's correlation id while live, `null` before it answers and after it exits. */
  runId: string | null;
  live: boolean;
  /** Between pressing Stop and the exit event — the console says "stopping" rather than
   *  looking like nothing happened while the tree is being killed. */
  stopping: boolean;
  finished: boolean;
  /** The exit code, or null when the process was killed / died without one — which is why
   *  `finished` is its own flag and not `exitCode !== null`. */
  exitCode: number | null;
  durationMs: number | null;
  startedAt: number;
  /** What produced it, so this tab's ⟳ repeats THIS run rather than the most recent one. */
  spec: RunSpec;
}

/** How many finished runs are kept. Beyond this the oldest closes itself: a console is a
 *  recent history, and thirty tabs is a filing cabinet nobody opens. */
const MAX_TABS = 8;

let tabSeq = 0;

function createBennuRunStore() {
  let building = $state(false);
  // The tool the last build ran with (`mvn` | `javac`) + whether it succeeded.
  let tool = $state('');
  let ok = $state<boolean | null>(null);
  let diagnostics = $state<BuildDiagnostic[]>([]);
  // When those diagnostics were produced (epoch ms). A compiler diagnostic is a statement about the
  // text the compiler read; the moment a file's buffer moves past this, its marks are describing
  // something that is no longer there — and they DRIFT, because their line/column are re-mapped
  // against the current buffer. One stale `cannot find symbol` rode an edit onto an unrelated
  // method and read as a false positive on code that compiles.
  let diagnosticsAt = $state(0);
  let lines = $state<RunLogLine[]>([]);

  // ── the Run console: one tab per run ────────────────────────────────────────
  let tabs = $state<RunTab[]>([]);
  let activeTabId = $state<string | null>(null);
  /** Events that named a run id no tab carried yet, by run id. See {@link stash}. */
  const orphans = new Map<string, ((tabId: string) => void)[]>();

  const activeTab = $derived(tabs.find((t) => t.id === activeTabId) ?? null);
  /** Every run in flight. A list, not a find: several programs can be running at once, each
   *  with its own tab, its own stdin and its own Stop. */
  const liveTabs = $derived(tabs.filter((t) => t.live));
  const running = $derived(liveTabs.length > 0);

  // Whole-project validation (the split-button's `validate` build type).
  let validating = $state(false);
  // The root of the in-flight validation, so `cancelValidation()` needs no argument.
  let validatingRoot = '';
  let validationResult = $state<ProjectValidationResult | null>(null);
  let validateProgress = $state<{ done: number; total: number } | null>(null);
  // Which build the split-button runs by default (and Ctrl+F9). Loaded from bennu config on attach.
  let preferredBuildType = $state<BuildType>('mvn');

  let attached = false;
  let unlisteners: UnlistenFn[] = [];

  /** Append to the BUILD log (mvn/javac/validation). `log` is the backend's interpretation
   *  of the line, absent on the status lines this store writes itself. */
  function push(text: string, stream: RunLogLine['stream'] = 'out', log?: LogAnnotation) {
    const next = lines.length >= MAX_LINES ? lines.slice(lines.length - MAX_LINES + 1) : lines.slice();
    next.push({ text, stream, ...log });
    lines = next;
  }

  /**
   * Replace `id`'s tab with a patched copy.
   *
   * A copy, not a mutation: the panel reads a tab through a `$derived` that ends in
   * `tabs.find(…)`, and a derived whose value is `===` its previous one propagates nothing —
   * so an in-place edit reaches the state and never the screen.
   */
  function patchTab(id: string, patch: Partial<RunTab>) {
    const i = tabs.findIndex((t) => t.id === id);
    if (i === -1) return;
    const next = tabs.slice();
    next[i] = { ...next[i], ...patch };
    tabs = next;
  }

  /** Append a line to a tab's transcript, capped. */
  function pushTo(
    id: string,
    text: string,
    stream: RunLogLine['stream'] = 'out',
    log?: LogAnnotation,
  ) {
    const i = tabs.findIndex((t) => t.id === id);
    if (i === -1) return;
    const prev = tabs[i].lines;
    const kept = prev.length >= MAX_LINES ? prev.slice(prev.length - MAX_LINES + 1) : prev.slice();
    kept.push({ text, stream, ...log });
    patchTab(id, { lines: kept });
  }

  /** Append to the ACTIVE tab — what the launch narration and the errors use. */
  function pushRun(text: string, stream: RunLogLine['stream'] = 'out') {
    if (activeTabId) pushTo(activeTabId, text, stream);
  }

  /** The tab holding that backend run id, or null while the launch that will claim it is still
   *  in flight — see {@link stash}. */
  function tabForRun(backendId: string): RunTab | null {
    return tabs.find((t) => t.runId === backendId) ?? null;
  }

  /**
   * Hold an event that arrived before its tab knew its run id, to be replayed by {@link claimRun}.
   *
   * A program can print (or exit) between the backend spawning it and `bennu_run` returning the id
   * to us. With one run at a time that window could be papered over by sending the stray output to
   * "the tab we just opened"; with several launches in flight that guess picks the wrong console,
   * so the id is waited for instead. Keyed by run id, so nothing has to be guessed at all.
   */
  function stash(runId: string, replay: (tabId: string) => void) {
    const queue = orphans.get(runId) ?? [];
    queue.push(replay);
    orphans.set(runId, queue);
  }

  /**
   * Bring a tab to the front, and with it its debug session.
   *
   * The console tab and the debug session are the same id, so "which run am I reading" and "which
   * session does the Debug panel show" are one question — asked here, in the one place that knows
   * the answer changed. The edge is one-way: the debugger knows nothing about tabs.
   */
  function focusTab(id: string | null) {
    activeTabId = id;
    bennuDebugStore.view(tabs.find((t) => t.id === id)?.runId ?? null);
  }

  /** Bind a tab to its backend run id and replay whatever arrived before it was known. */
  function claimRun(tabId: string, runId: string, patch: Partial<RunTab> = {}) {
    patchTab(tabId, { ...patch, runId });
    // The tab was opened before the id existed, so its session could not be pointed at then.
    if (tabId === activeTabId) bennuDebugStore.view(runId);
    const queue = orphans.get(runId);
    if (!queue) return;
    orphans.delete(runId);
    for (const replay of queue) replay(tabId);
  }

  /** The program in `tabId` ended — close the tab's record of it and say how it went. */
  function exited(tabId: string, code: number | null) {
    const tab = tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const durationMs = tab.startedAt ? Date.now() - tab.startedAt : null;
    patchTab(tabId, {
      live: false,
      runId: null,
      finished: true,
      durationMs,
      // A process we killed reports whatever the kill produced (`taskkill /F` gives 1),
      // which is not the program's verdict on itself — so a stopped run has no code, and
      // the console says "Stopped" instead of inventing a failure.
      exitCode: tab.stopping ? null : code,
      stopping: false,
    });
    // A killed process has no exit code of its own to report; saying "terminated" is
    // both true and the answer to "did my Stop work".
    pushTo(
      tabId,
      tab.stopping
        ? `Process terminated${durationMs === null ? '' : ` after ${formatMs(durationMs)}`}.`
        : `Process finished with exit code ${code ?? '?'}` +
          `${durationMs === null ? '' : ` in ${formatMs(durationMs)}`}`,
      tab.stopping || code !== 0 ? 'err' : 'meta',
    );
  }

  /** Attach the build/run event listeners. Called once from BennuWindow.onMount;
   *  returns a detach fn for cleanup. Idempotent. */
  async function attach(): Promise<UnlistenFn> {
    if (attached) return detach;
    attached = true;
    // Load the preferred build type once (filesystem config, per rule 11 — not localStorage).
    getBennuConfig()
      .then((cfg) => {
        preferredBuildType = cfg.preferred_build_type === 'validate' ? 'validate' : 'mvn';
      })
      .catch(() => {
        /* missing/corrupt config → keep the default */
      });
    const add = (f: UnlistenFn) => unlisteners.push(f);
    add(
      await listen<{ text: string } & LogAnnotation>('arbor://bennu/build-output', (e) =>
        push(e.payload.text, 'out', { level: e.payload.level, pieces: e.payload.pieces }),
      ),
    );
    add(
      await listen<{ done: number; total: number }>('arbor://bennu/validate-progress', (e) => {
        validateProgress = { done: e.payload.done, total: e.payload.total };
      }),
    );
    add(
      await listen<{ run_id: string; stream: string; text: string } & LogAnnotation>(
        'arbor://bennu/run-output',
        (e) => {
          const write = (tabId: string) =>
            pushTo(tabId, e.payload.text, e.payload.stream === 'stderr' ? 'err' : 'out', {
              level: e.payload.level,
              pieces: e.payload.pieces,
            });
          const tab = tabForRun(e.payload.run_id);
          if (tab) write(tab.id);
          else stash(e.payload.run_id, write);
        },
      ),
    );
    add(
      await listen<{ run_id: string; code: number | null }>('arbor://bennu/run-exit', (e) => {
        const tab = tabForRun(e.payload.run_id);
        if (tab) exited(tab.id, e.payload.code);
        else stash(e.payload.run_id, (tabId) => exited(tabId, e.payload.code));
      }),
    );
    add(
      // A breakpoint firing in a program you were not reading pulls the console to it, the way it
      // pulls the whole window forward (`surface`). The session id IS the run id, so the tab is a
      // lookup — and the Debug panel follows the tab, which keeps the two agreeing about which
      // program is being looked at.
      await listen<{ session_id: string }>('arbor://bennu/debug-paused', (e) => {
        const tab = tabForRun(e.payload.session_id);
        if (tab && tab.id !== activeTabId) focusTab(tab.id);
      }),
    );
    return detach;
  }
  function detach() {
    for (const f of unlisteners) f();
    unlisteners = [];
    attached = false;
  }

  /** Compile the project. Opens the Build dock (unless `focus` is false — a compile that is
   *  the first half of a launch belongs on the Run console, not on Maven's log), streams the
   *  log, resolves with the parsed result (or null on a hard failure). No-op while already
   *  building. */
  async function build(root: string, focus = true, module = ''): Promise<BuildResult | null> {
    if (building) return null;
    building = true;
    ok = null;
    tool = '';
    diagnostics = [];
    lines = [];
    if (focus) bennuUiStore.showBottom('build');
    push(module ? `Compiling ${module}…` : `Compiling ${root}…`, 'meta');
    try {
      const res = await ipcBuild(root, module);
      tool = res.tool;
      ok = res.ok;
      diagnostics = res.diagnostics;
      diagnosticsAt = Date.now();
      push(
        res.ok
          ? `Build succeeded (${res.tool}).`
          : `Build failed (${res.tool}) — ${res.diagnostics.length} problem(s).`,
        res.ok ? 'meta' : 'err',
      );
      return res;
    } catch (e) {
      ok = false;
      push(`Build error: ${e instanceof Error ? e.message : String(e)}`, 'err');
      return null;
    } finally {
      building = false;
    }
  }

  /** Validate the WHOLE project without compiling (the `validate` build type): stream progress,
   *  collect timing stats + diagnostics. Opens the Build dock. No-op while already busy. Shares the
   *  BE single-run guard with the Maven build, so a concurrent start is refused there too. */
  async function validateProject(root: string): Promise<ProjectValidationResult | null> {
    if (building || validating) return null;
    validating = true;
    validatingRoot = root;
    validationResult = null;
    validateProgress = { done: 0, total: 0 };
    lines = [];
    bennuDiagnosticsStore.clearProjectDiagnostics();
    bennuUiStore.showBottom('build');
    push(`Validating ${root} (no compile)…`, 'meta');
    try {
      const res = await ipcValidateProject(root);
      validationResult = res;
      // Route the per-file diagnostics to the Problems panel (where problems belong).
      bennuDiagnosticsStore.setProjectDiagnostics(res.diagnostics);
      push(
        `Validated ${res.total_files} file(s) in ${formatMs(res.total_ms)} — ` +
          `${res.error_count} error(s), ${res.warning_count} warning(s). ` +
          `avg ${res.avg_ms.toFixed(1)}ms, max ${res.max_ms}ms` +
          (res.max_file ? ` (${res.max_file.split('/').pop()})` : ''),
        res.error_count > 0 ? 'err' : 'meta',
      );
      return res;
    } catch (e) {
      push(`Validation error: ${e instanceof Error ? e.message : String(e)}`, 'err');
      return null;
    } finally {
      validating = false;
      validateProgress = null;
    }
  }

  /** Cancel the running whole-project validation (the Build panel's Cancel while validating). The BE
   *  stops the sweep and discards its partial results; the in-flight `validateProject` promise then
   *  resolves with an empty result. Best-effort. */
  async function cancelValidation(): Promise<void> {
    if (!validating) return;
    try {
      await ipcCancelValidation(validatingRoot);
    } catch {
      /* best-effort — the BE may already have finished */
    }
  }

  /** Persist the preferred build type (filesystem config) and update the reactive state. Merges into
   *  the existing config so the other bennu settings are preserved. */
  async function setPreferredBuildType(type: BuildType): Promise<void> {
    preferredBuildType = type;
    try {
      const cfg = await getBennuConfig();
      await setBennuConfig({ ...cfg, preferred_build_type: type });
    } catch {
      /* best-effort persistence — the in-memory choice still applies this session */
    }
  }

  /**
   * Run the preferred build type for `root` (the split-button main action + Ctrl+F9).
   *
   * "Validate (no compile)" is the whole-project **Java** analysis sweep, so on a Cargo
   * project it is not an alternative — it is a different language's feature. The
   * preference is per-profile, not per-project, so a user who left it on `validate` while
   * working on a Java tree would otherwise press Ctrl+F9 on a Rust one and get an empty
   * sweep instead of `cargo check`. The stored preference is left untouched: it still
   * applies to the Java projects it was chosen for.
   */
  async function runPreferred(root: string): Promise<void> {
    if (preferredBuildType === 'validate' && !projectStore.isCargo) {
      await validateProject(root);
    } else {
      await build(root);
    }
  }

  /**
   * Build, then — if the compile is clean — launch the program, with its output going to
   * the Run console.
   *
   * The compile is part of the launch, so the console narrates it ("Compiling…", and what
   * went wrong if it did) while Maven's own log goes where build logs go. Being sent to the
   * Build panel and then having to come back is the reason a Run tool window exists at all.
   */
  /**
   * Open a console tab for a launch about to happen, and return its id.
   *
   * Shared by both launchers because everything here is about the *console* rather than about what
   * is being run: the tab strip's capacity, which tab is in front, and the window that output can
   * arrive in before the backend has answered with a run id.
   */
  function openTab(spec: RunSpec, label: string, subject: string): string {
    tabSeq += 1;
    const id = `rt-${tabSeq}`;
    const tab: RunTab = {
      id,
      label,
      subject,
      command: '',
      workingDir: '',
      lines: [],
      runId: null,
      live: false,
      stopping: false,
      finished: false,
      exitCode: null,
      durationMs: null,
      startedAt: 0,
      spec,
    };
    // Oldest FINISHED tabs go first — never the one that is still running.
    const kept = tabs.slice();
    while (kept.length >= MAX_TABS) {
      const victim = kept.findIndex((t) => !t.live);
      if (victim === -1) break;
      kept.splice(victim, 1);
    }
    tabs = [...kept, tab];
    focusTab(id);
    bennuUiStore.showBottom('run');
    return id;
  }

  async function launch(spec: JvmRunSpec): Promise<void> {
    // Only a compile in flight refuses a launch, and not as a policy: `bennu_build` takes the
    // backend's build guard, so a second compile would be refused there anyway. A program already
    // RUNNING is no reason at all — a server and the client that talks to it are two runs.
    if (building) return;
    const cls = spec.mainClass.trim();
    if (!cls) return;

    // A new tab, not a cleared buffer: the previous run stays readable beside this one.
    const id = openTab(spec, spec.label || cls.split('.').pop() || cls, cls);
    pushTo(id, 'Compiling…', 'meta');

    // Only the module being run (and what it is built from) — see `bennu_build`'s `module`.
    const res = await build(spec.root, false, spec.module);
    if (res?.ok && res.tool === 'up-to-date') {
      // Worth saying: the launch was instant because there was nothing to compile, not
      // because the compile was skipped by accident.
      pushTo(id, 'Up to date — nothing to compile.', 'meta');
    }
    if (!res || !res.ok) {
      const n = res?.diagnostics.length ?? 0;
      pushTo(
        id,
        n > 0
          ? `Build failed — ${n} problem(s). The Build panel has the log.`
          : 'Build failed. The Build panel has the log.',
        'err',
      );
      patchTab(id, { finished: true });
      return;
    }

    patchTab(id, { live: true, startedAt: Date.now() });
    try {
      const handle = await ipcRun(spec.root, cls, {
        module: spec.module,
        args: spec.args,
        vmArgs: spec.vmArgs,
        workingDir: spec.workingDir,
        env: spec.env,
        debug: spec.debug,
        debugSuspend: spec.debugSuspend,
        classpathScope: spec.classpathScope,
      });
      claimRun(id, handle.run_id, {
        command: handle.command,
        workingDir: handle.working_dir,
      });
    } catch (e) {
      patchTab(id, { live: false, finished: true });
      pushTo(id, `Could not start: ${e instanceof Error ? e.message : String(e)}`, 'err');
    }
  }

  /**
   * Launch a script, streaming into the Run console.
   *
   * No compile step and no build guard: a script is not built, and it is not what serializes on
   * `target/` either. The refusals — a `.bat` on a Mac, a `.sh` on a Windows box with no Git Bash
   * — arrive from the backend as an error, and are printed into the tab rather than swallowed into
   * a toast: the answer names a program and a place to install it, and that is worth keeping on
   * screen next to what you tried to run.
   */
  async function launchScript(spec: ScriptRunSpec): Promise<void> {
    const name = spec.file.split(/[\\/]/).pop() ?? spec.file;
    const id = openTab(spec, spec.label || name, name);
    patchTab(id, { live: true, startedAt: Date.now() });
    try {
      const handle = await ipcRunScript(spec.root, spec.file, {
        args: spec.args,
        workingDir: spec.workingDir,
        env: spec.env,
      });
      claimRun(id, handle.run_id, { command: handle.command, workingDir: handle.working_dir });
    } catch (e) {
      patchTab(id, { live: false, finished: true });
      pushTo(id, `Could not start: ${e instanceof Error ? e.message : String(e)}`, 'err');
    }
  }

  /**
   * Launch a cargo command, streaming into the Run console.
   *
   * No compile step, and that is the whole difference from {@link launch}: a cargo subcommand *is*
   * the build, so prefixing it with one would compile the workspace twice. The console narrates the
   * command instead, since `cargo check` on a cold workspace is a minute of silence otherwise.
   */
  async function launchCargo(spec: CargoRunSpec): Promise<void> {
    // See `launch`: the compile is what serializes, not the running.
    if (building) return;
    const id = openTab(spec, spec.label, spec.invocation.command);
    patchTab(id, { live: true, startedAt: Date.now() });
    try {
      // The one difference: a debug launch has to build first, because a native binary cannot be
      // debugged by starting it differently the way a JVM can — the debugger has to be what starts it.
      const handle = spec.debug
        ? await ipcCargoDebug(spec.root, spec.invocation, {
            workingDir: spec.workingDir,
            env: spec.env,
          })
        : await ipcCargoRun(spec.root, spec.invocation, {
            workingDir: spec.workingDir,
            env: spec.env,
          });
      claimRun(id, handle.run_id, {
        command: handle.command,
        workingDir: handle.working_dir,
      });
    } catch (e) {
      patchTab(id, { live: false, finished: true });
      // The backend's message carries the install hint when a rustup component is what is missing —
      // which is the difference between "clippy is broken" and "clippy is not installed".
      pushTo(id, `Could not start: ${e instanceof Error ? e.message : String(e)}`, 'err');
    }
  }

  /**
   * Launch a named run configuration — every field of it, which is the difference between a
   * run configuration and a main class.
   *
   * The one place that knows a configuration's KIND decides who runs it: an application
   * goes to the JVM launcher and the Run console, a `junit` one to the test runner and the
   * Tests panel. Callers (▷, the selector, the editor) say "run this configuration" and are
   * not each required to know the difference.
   */
  async function runConfig(root: string, cfg: RunConfig, debug = false): Promise<void> {
    if (!isRunKind(cfg.kind)) {
      // Written by a newer Bennu. Say so rather than launching some approximation of it.
      bennuUiStore.showBottom('run');
      pushRun(`“${cfg.name}” is a ${cfg.kind} configuration, which this version cannot run.`, 'err');
      return;
    }
    if (cfg.kind === 'cargo') {
      // Debugging a cargo configuration is the configuration you have, with a debugger. The backend
      // builds the target first and launches the binary under an adapter — `cargo run` cannot be
      // debugged directly, because it execs the program itself and never hands anyone a path.
      await launchCargo({
        kind: 'cargo',
        root,
        invocation: cargoInvocationOf(cfg),
        workingDir: cfg.workingDir.trim(),
        env: envRecord(cfg.env),
        label: debug ? `Debug ${cfg.name}` : cfg.name,
        debug,
      });
      return;
    }
    if (cfg.kind === 'script') {
      const file = cfg.scriptFile.trim();
      if (!file) {
        bennuUiStore.showBottom('run');
        pushRun(`“${cfg.name}” has no script to run — name one in the configuration.`, 'err');
        return;
      }
      if (debug) {
        // There is no debugger to attach here: a shell script's "debugger" is `set -x`, and
        // pretending otherwise by running it plainly under a Debug button would be a lie about
        // what just happened.
        bennuUiStore.showBottom('run');
        pushRun('A script configuration cannot be debugged — running it instead.', 'err');
      }
      await launchScript({
        kind: 'script',
        root,
        file,
        args: splitArgs(cfg.programArgs),
        workingDir: cfg.workingDir.trim(),
        env: envRecord(cfg.env),
        label: cfg.name,
      });
      return;
    }
    if (cfg.kind === 'junit') {
      if (debug) {
        // Maven forks its own JVM for Surefire, so the agent would have to go through
        // `argLine` rather than onto our command line — a different launch path, not a flag.
        // Saying so beats silently running the tests without a debugger attached.
        bennuUiStore.showBottom('run');
        pushRun('Debugging a JUnit configuration is not supported yet — running it instead.', 'err');
      }
      await bennuTestStore.run(root, testScopeOf(cfg));
      return;
    }
    // A Spring Boot configuration need not name its class: a Boot module has exactly one
    // `@SpringBootApplication`, so asking would be asking a question with one answer.
    const cls = cfg.mainClass.trim() || (await bootClassOf(root, cfg));
    if (!cls) {
      bennuUiStore.showBottom('run');
      pushRun(
        cfg.kind === 'springboot'
          ? 'No @SpringBootApplication class found in this module — name one in the configuration.'
          : 'This configuration has no main class.',
        'err',
      );
      return;
    }
    await launch({
      kind: 'jvm',
      root,
      module: cfg.module.trim(),
      mainClass: cls,
      args: splitArgs(cfg.programArgs),
      vmArgs: vmArgsOf(cfg),
      workingDir: cfg.workingDir.trim(),
      env: envRecord(cfg.env),
      label: cfg.name,
      debug,
      // Only meaningful under the debugger, and only when the configuration asked: a launch
      // that begins frozen is the exception, not the default.
      debugSuspend: debug && cfg.debugSuspend,
      classpathScope: cfg.classpathScope,
    });
  }

  /** The `@SpringBootApplication` entry point of `cfg`'s module, when there is exactly one.
   *  Empty for anything else — including two of them, which is a real ambiguity and not
   *  something to resolve by picking the first. */
  async function bootClassOf(root: string, cfg: RunConfig): Promise<string> {
    if (cfg.kind !== 'springboot') return '';
    const found = (await bennuMainClassStore.load(root)).filter(
      (m) => m.spring_boot && (!cfg.module.trim() || m.module === cfg.module.trim()),
    );
    return found.length === 1 ? found[0].fqcn : '';
  }

  /**
   * The VM arguments a configuration launches with: what was typed, plus — for a Spring Boot
   * one — its profiles as `-Dspring.profiles.active=…`.
   *
   * Appended rather than merged: a typed `-Dspring.profiles.active` would be overridden by
   * the later one, so whoever filled in the Profiles field wins over whoever left a stale
   * value in VM arguments. Adding it here and not in the field itself keeps the two
   * separately editable, which is the reason the field exists.
   */
  function vmArgsOf(cfg: RunConfig): string[] {
    const args = splitArgs(cfg.vmArgs);
    const profiles = cfg.profiles.trim();
    if (cfg.kind === 'springboot' && profiles) {
      args.push(`-Dspring.profiles.active=${profiles}`);
    }
    return args;
  }

  /** A `junit` configuration's two fields as the runner's scope. An empty target degrades to
   *  "everything" rather than running a scope that names nothing. */
  function testScopeOf(cfg: RunConfig): TestScope {
    const target = cfg.testTarget.trim();
    if (!target) return { kind: 'all' };
    if (cfg.testScope === 'module') return { kind: 'module', module: target };
    if (cfg.testScope === 'class') return { kind: 'classes', classes: [target] };
    return { kind: 'all' };
  }

  /**
   * Run the ACTIVE run configuration for `root` (the titlebar ▶ / Shift+F10 path).
   *
   * With no configuration yet, it does not give up: it scans the project for entry points
   * and, when there is exactly ONE, creates the configuration and runs it. A project with a
   * single `main` needing you to open an editor and type its fully-qualified name before ▶
   * does anything is a button that does not work. Several entry points is a real question,
   * so that returns false and the caller opens the editor.
   */
  async function runActive(root: string, debug = false): Promise<boolean> {
    const cfg = bennuRunConfigStore.activeFor(root);
    // A Spring Boot configuration is runnable without a class of its own — `runConfig`
    // resolves the module's `@SpringBootApplication`. So is a Cargo one, which has a command
    // rather than a class.
    if (cfg && (cfg.mainClass.trim() || cfg.kind !== 'application')) {
      await runConfig(root, cfg, debug);
      return true;
    }
    if (bennuRunConfigStore.configsFor(root).length) return false;

    // A Cargo project has no `main` class to scan for; its entry points are the workspace's binary
    // targets. One of them is an answer; several is a real ambiguity `cargo run` itself refuses to
    // resolve, so the editor opens instead of launching the wrong program.
    if (projectStore.isCargo) return await runSoleCargoBinary(root, debug);

    const found = await bennuMainClassStore.load(root);
    if (found.length !== 1) return false;
    const only = found[0];
    // A Boot entry point makes a Spring Boot configuration, not a bare Application — so its
    // Profiles field is there the first time you go looking for it.
    const id = bennuRunConfigStore.create(
      root,
      only.spring_boot ? 'springboot' : 'application',
      {
        name: only.fqcn.split('.').pop() || only.fqcn,
        mainClass: only.fqcn,
        module: only.module ?? '',
      },
    );
    bennuRunConfigStore.setActive(root, id);
    const created = bennuRunConfigStore.activeFor(root);
    if (created) await runConfig(root, created, debug);
    return true;
  }

  /**
   * Create and run a Cargo configuration for the workspace's ONLY binary, if there is one.
   *
   * The Cargo counterpart of the single-`main` shortcut above, and it exists for the same reason: a
   * project with one binary should not need you to open a form and fill in a crate name before ▶
   * does anything. Returns false when there is no single answer, and the caller opens the editor.
   */
  async function runSoleCargoBinary(root: string, debug: boolean): Promise<boolean> {
    const ws = await cargoWorkspace(root).catch(() => null);
    if (!ws) return false;
    const bins = ws.crates.flatMap((c) =>
      c.targets.filter((t) => t.kind === 'bin').map((t) => ({ crate: c.name, target: t.name })),
    );
    if (bins.length !== 1) return false;
    const only = bins[0];
    const id = bennuRunConfigStore.create(root, 'cargo', {
      name: only.target,
      module: only.crate,
      cargoCommand: 'run',
      cargoTargetKind: 'bin',
      cargoTarget: only.target,
      cargoWorkspace: false,
    });
    bennuRunConfigStore.setActive(root, id);
    const created = bennuRunConfigStore.activeFor(root);
    if (created) await runConfig(root, created, debug);
    return true;
  }

  /**
   * Repeat a run, exactly — the ⟳ on a tab repeats THAT tab's run, not the most recent one.
   * Without a tab id it repeats the one you are looking at, which is what the header's ⟳
   * means. The result is a new tab: a rerun is another run, and overwriting the transcript
   * you were comparing against is the thing tabs exist to prevent.
   */
  async function rerunApp(tabId?: string): Promise<void> {
    const from = tabId ? tabs.find((t) => t.id === tabId) : activeTab;
    if (!from) return;
    if (running) await stop();
    if (from.spec.kind === 'cargo') {
      await launchCargo(from.spec);
      return;
    }
    if (from.spec.kind === 'script') {
      await launchScript(from.spec);
      return;
    }
    await launch(from.spec);
  }

  /**
   * The run a console verb acts on: the tab in front when it is live, else the only live one.
   *
   * The tab in front first, because the console shows one transcript and Stop under it means that
   * program. `null` rather than a guess when several are running and none of them is the one being
   * looked at — killing whichever happened to be first in the list is not a thing to do to a
   * program by accident.
   */
  function targetTab(): RunTab | null {
    if (activeTab?.live) return activeTab;
    return liveTabs.length === 1 ? liveTabs[0] : null;
  }

  /** Feed one line to the running program's stdin, echoing it into its tab — a terminal
   *  echoes what you type, and without it the transcript reads as the program answering
   *  questions nobody asked. */
  async function sendInput(text: string): Promise<void> {
    const tab = targetTab();
    if (!tab?.runId) return;
    pushTo(tab.id, text, 'in');
    try {
      await ipcRunInput(tab.runId, text);
    } catch (e) {
      pushTo(
        tab.id,
        `Could not write to the program: ${e instanceof Error ? e.message : String(e)}`,
        'err',
      );
    }
  }

  /** Stop the run in front — the backend kills the whole process tree. The tab stays `live`
   *  until the exit event lands, so the console shows "stopping…" instead of claiming the
   *  program is gone while it is still winding down. See {@link targetTab} for which run. */
  async function stop(): Promise<void> {
    const tab = targetTab();
    if (!tab?.runId) return;
    patchTab(tab.id, { stopping: true });
    try {
      await ipcCancelRun(tab.runId);
    } catch {
      // The exit event may already be in flight; if it never comes, the flag is cleared by
      // the next launch.
    }
  }

  /** Close a tab. A LIVE one is stopped first — closing the only window onto a program you
   *  started, and leaving it running, is how an orphan happens. */
  async function closeTab(id: string): Promise<void> {
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    if (tab.live && tab.runId) {
      try {
        await ipcCancelRun(tab.runId);
      } catch {
        /* best-effort — it is going away either way */
      }
    }
    const i = tabs.findIndex((t) => t.id === id);
    const next = tabs.filter((t) => t.id !== id);
    tabs = next;
    if (activeTabId === id) {
      // The neighbour, the way every tab strip does it.
      focusTab((next[i] ?? next[i - 1] ?? next[next.length - 1] ?? null)?.id ?? null);
    }
  }

  return {
    get building() { return building; },
    get running() { return running; },
    get validating() { return validating; },
    /**
     * Busy = a compile or a validation is in flight — what disables the build / ▷ buttons.
     *
     * A live RUN is deliberately not part of it. Those two share the backend's build guard and
     * genuinely cannot overlap; a running program cannot stop you starting another one, which is
     * the whole point of a console with several tabs. Ask {@link running} for that.
     */
    get active() { return building || validating; },
    /** Every run in flight — what the console's tab strip marks live. */
    get liveTabs() { return liveTabs; },
    /** Whether Stop / stdin have an unambiguous target: see `targetTab`. */
    get canStop() { return targetTab() !== null; },
    get stopping() { return activeTab?.stopping ?? false; },
    get tool() { return tool; },
    get ok() { return ok; },
    get diagnostics() { return diagnostics; },
    /** When {@link diagnostics} were produced (epoch ms); 0 when no build has run. */
    get diagnosticsAt() { return diagnosticsAt; },
    /** The BUILD log (mvn / javac / validation). */
    get lines() { return lines; },

    // ── the Run console ───────────────────────────────────────────────────────
    // One tab per run. The singular getters below are the ACTIVE tab's — every consumer
    // outside the console itself is asking about the run you are looking at.
    /** Every run, oldest first — the console's tab strip. */
    get tabs() { return tabs; },
    get activeTabId() { return activeTabId; },
    get activeTab() { return activeTab; },
    /** Whether the ACTIVE tab is the one currently running. */
    get activeIsLive() { return activeTab?.live ?? false; },
    /** The active tab's output. */
    get runLines() { return activeTab?.lines ?? []; },
    /** The command line actually spawned (from the backend). */
    get runCommand() { return activeTab?.command ?? ''; },
    get runWorkingDir() { return activeTab?.workingDir ?? ''; },
    /** The run configuration's name, or the class for an ad-hoc launch. */
    get runLabel() { return activeTab?.label ?? ''; },
    get runExitCode() { return activeTab?.exitCode ?? null; },
    /** True once the active run has exited (however it exited). */
    get runFinished() { return activeTab?.finished ?? false; },
    get runDurationMs() { return activeTab?.durationMs ?? null; },
    /** Whether there is a run to repeat — any tab will do, ⟳ repeats the active one. */
    get canRerun() { return activeTab !== null; },
    /** Whether the console has anything to show at all. */
    get hasRunOutput() { return tabs.length > 0; },
    get validationResult() { return validationResult; },
    get validateProgress() { return validateProgress; },
    get preferredBuildType() { return preferredBuildType; },

    attach,
    build,
    validateProject,
    cancelValidation,
    runPreferred,
    setPreferredBuildType,
    runConfig,
    /**
     * Run a cargo command that is not a saved configuration — what the Cargo tool window's rows do.
     *
     * Ad-hoc on purpose: clicking `check` on a crate should not leave a configuration behind for
     * every crate you have ever checked. The tab still carries the invocation, so ⟳ repeats it.
     */
    /**
     * Run one main class, without a saved configuration — what the ▶ in the editor's gutter does.
     *
     * Ad-hoc for the same reason `runCargoCommand` is: pressing ▶ beside a `main` means "run
     * this, now", and leaving a configuration behind for every class anyone has ever glanced at
     * would turn the configuration list into a history of clicks. A run worth keeping is one you
     * name, in the dialog that exists for it.
     *
     * `runtime` scope and no arguments, which is what an unconfigured run *is*: a class needing
     * VM flags or a working directory is a class needing a configuration, and the gutter's menu
     * says so by offering to make one.
     */
    async runMainClass(
      root: string,
      opts: { mainClass: string; module?: string; label?: string; debug?: boolean },
    ): Promise<void> {
      await launch({
        kind: 'jvm',
        root,
        module: opts.module ?? '',
        mainClass: opts.mainClass,
        args: [],
        vmArgs: [],
        workingDir: '',
        env: {},
        label: opts.label ?? opts.mainClass.split('.').pop() ?? opts.mainClass,
        debug: !!opts.debug,
        debugSuspend: false,
        classpathScope: 'runtime',
      });
    },

    /**
     * Run a script file — what ▶ beside a `.sh` / `.cmd` / `.ps1` does.
     *
     * Ad-hoc like {@link runMainClass}: no arguments, no environment, the script's own directory.
     * A script that needs any of those is one you want a configuration for.
     */
    async runScript(root: string, file: string, opts: { args?: string[] } = {}): Promise<void> {
      await launchScript({
        kind: 'script',
        root,
        file,
        args: opts.args ?? [],
        workingDir: '',
        env: {},
        label: file.split(/[\\/]/).pop() ?? file,
      });
    },

    async runCargoCommand(
      root: string,
      invocation: CargoInvocation,
      label: string,
      opts: { workingDir?: string; env?: Record<string, string>; debug?: boolean } = {},
    ): Promise<void> {
      await launchCargo({
        kind: 'cargo',
        root,
        invocation,
        label,
        workingDir: opts.workingDir ?? '',
        env: opts.env ?? {},
        // Under the debugger the binary is built first and launched by the adapter — see
        // `launchCargo`. Set by the ▶'s sibling lens (`rust-analyzer.debugSingle`).
        debug: opts.debug ?? false,
      });
    },
    runActive,
    rerunApp,
    sendInput,
    stop,
    closeTab,
    /** Show a tab. */
    showTab(id: string) {
      if (tabs.some((t) => t.id === id)) focusTab(id);
    },
    /** Clear the BUILD log + last result (the Build panel's "clear" action). */
    clear() {
      lines = [];
      diagnostics = [];
      ok = null;
      tool = '';
      validationResult = null;
    },
    /** Close every FINISHED run (the console's 🗑). The live one stays — clearing the console
     *  is a tidy-up, not a way to kill a program. */
    clearRun() {
      const live = tabs.filter((t) => t.live);
      tabs = live;
      focusTab(live[0]?.id ?? null);
    },
  };
}

export const bennuRunStore = createBennuRunStore();
