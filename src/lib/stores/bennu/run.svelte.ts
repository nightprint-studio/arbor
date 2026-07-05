/**
 * Bennu run/build store — drives the titlebar Run/Build controls and the bottom
 * Build tool window.
 *
 * The backend (`bennu-be`, `build.rs`) does the real work:
 *   • `bennu_build`  → `mvn -q -o compile` (offline, project JDK) with a `javac`
 *     fallback; the raw log streams as `arbor://bennu/build-output`, the resolved
 *     promise carries the parsed {@link BuildResult} (tool · ok · diagnostics). A
 *     clean build re-indexes `target/classes` on the BE.
 *   • `bennu_run`    → `java -cp target/classes:deps <mainClass>`; stdout/stderr
 *     stream as `arbor://bennu/run-output`, ending with `arbor://bennu/run-exit`.
 *   • `bennu_cancel_run` → stop a live run by id.
 *
 * This store owns the FE-side lifecycle: the streamed log buffer, the parsed
 * diagnostics, the building/running flags, the last main class per project (there's
 * no main-class discovery yet — the run-config modal supplies it), and the Tauri
 * event subscription (attached once from the window's onMount). Rune store —
 * private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteMap } from 'svelte/reactivity';
import {
  build as ipcBuild, validateProject as ipcValidateProject, run as ipcRun,
  cancelRun as ipcCancelRun,
} from '$lib/ipc/bennu';
import { getBennuConfig, setBennuConfig } from '$lib/ipc/bennu/config';
import type { BuildResult, BuildDiagnostic, ProjectValidationResult } from '$lib/types/bennu';
import { bennuUiStore } from './ui.svelte';
import { bennuDiagnosticsStore } from './diagnostics.svelte';
import { bennuRunConfigStore, splitArgs } from './run-config.svelte';

/** The two build kinds the split-button offers. */
export type BuildType = 'mvn' | 'validate';

/** One streamed log line + which channel it came from (drives colouring). */
export interface RunLogLine {
  text: string;
  /** `out` = stdout / mvn log · `err` = stderr · `meta` = our own status lines. */
  stream: 'out' | 'err' | 'meta';
}

// Cap the retained log so a chatty build/run can't grow the buffer unbounded.
const MAX_LINES = 3000;

/** Render a millisecond duration compactly (`340ms` / `1.2s` / `1m 05s`). */
export function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${String(Math.round(s % 60)).padStart(2, '0')}s`;
}

function createBennuRunStore() {
  let building = $state(false);
  let running = $state(false);
  // The tool the last build ran with (`mvn` | `javac`) + whether it succeeded.
  let tool = $state('');
  let ok = $state<boolean | null>(null);
  let diagnostics = $state<BuildDiagnostic[]>([]);
  let lines = $state<RunLogLine[]>([]);

  // Whole-project validation (the split-button's `validate` build type).
  let validating = $state(false);
  let validationResult = $state<ProjectValidationResult | null>(null);
  let validateProgress = $state<{ done: number; total: number } | null>(null);
  // Which build the split-button runs by default (and Ctrl+F9). Loaded from bennu config on attach.
  let preferredBuildType = $state<BuildType>('mvn');

  // The live run's correlation id (null when nothing is running). Not reactive —
  // only the event handlers + stop() read it.
  let runId: string | null = null;
  // Last main class used per project root (no discovery yet — the run-config modal
  // sets it, ▶ Run reuses it). SvelteMap so `mainClassFor` stays reactive.
  const mainClasses = new SvelteMap<string, string>();

  let attached = false;
  let unlisteners: UnlistenFn[] = [];

  function push(text: string, stream: RunLogLine['stream'] = 'out') {
    const next = lines.length >= MAX_LINES ? lines.slice(lines.length - MAX_LINES + 1) : lines.slice();
    next.push({ text, stream });
    lines = next;
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
      await listen<{ text: string }>('arbor://bennu/build-output', (e) => push(e.payload.text, 'out')),
    );
    add(
      await listen<{ done: number; total: number }>('arbor://bennu/validate-progress', (e) => {
        validateProgress = { done: e.payload.done, total: e.payload.total };
      }),
    );
    add(
      await listen<{ run_id: string; stream: string; text: string }>(
        'arbor://bennu/run-output',
        (e) => {
          if (runId && e.payload.run_id !== runId) return;
          push(e.payload.text, e.payload.stream === 'stderr' ? 'err' : 'out');
        },
      ),
    );
    add(
      await listen<{ run_id: string; code: number | null }>('arbor://bennu/run-exit', (e) => {
        if (runId && e.payload.run_id !== runId) return;
        running = false;
        runId = null;
        const code = e.payload.code;
        push(`Process finished with exit code ${code ?? '?'}`, code === 0 ? 'meta' : 'err');
      }),
    );
    return detach;
  }
  function detach() {
    for (const f of unlisteners) f();
    unlisteners = [];
    attached = false;
  }

  /** Compile the project. Opens the Build dock, streams the log, resolves with the
   *  parsed result (or null on a hard failure). No-op while already building. */
  async function build(root: string): Promise<BuildResult | null> {
    if (building) return null;
    building = true;
    ok = null;
    tool = '';
    diagnostics = [];
    lines = [];
    bennuUiStore.showBottom('build');
    push(`Compiling ${root}…`, 'meta');
    try {
      const res = await ipcBuild(root);
      tool = res.tool;
      ok = res.ok;
      diagnostics = res.diagnostics;
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

  /** Run the preferred build type for `root` (the split-button main action + Ctrl+F9). */
  async function runPreferred(root: string): Promise<void> {
    if (preferredBuildType === 'validate') {
      await validateProject(root);
    } else {
      await build(root);
    }
  }

  /** Build then, if the compile is clean, launch `mainClass` with optional program
   *  `args`. Remembers the class for this root so ▶ Run can reuse it. No-op while
   *  busy.
   *
   *  NOTE: VM args / working dir / env from a run config are NOT yet forwarded — the
   *  `bennu_run` BE handler only accepts `{ root, main_class, args }` today. Passing
   *  those through is a BE follow-up (see the run-config BE contract). */
  async function run(root: string, mainClass: string, args: string[] = []): Promise<void> {
    if (building || running) return;
    const cls = mainClass.trim();
    if (!cls) return;
    mainClasses.set(root, cls);
    const res = await build(root);
    if (!res || !res.ok) {
      push('Run aborted — fix the build first.', 'err');
      return;
    }
    running = true;
    push(`Running ${cls}${args.length ? ' ' + args.join(' ') : ''}…`, 'meta');
    try {
      const handle = await ipcRun(root, cls, args);
      runId = handle.run_id;
    } catch (e) {
      running = false;
      runId = null;
      push(`Run error: ${e instanceof Error ? e.message : String(e)}`, 'err');
    }
  }

  /** Run the ACTIVE run configuration for `root` (the titlebar ▶ / Shift+F10 path).
   *  Reads the active config from {@link bennuRunConfigStore} and launches its main
   *  class + program args. Returns false when there's no active config to run (the
   *  caller then opens the run-config editor to pick/create one). */
  async function runActive(root: string): Promise<boolean> {
    const cfg = bennuRunConfigStore.activeFor(root);
    if (!cfg || !cfg.mainClass.trim()) return false;
    await run(root, cfg.mainClass, splitArgs(cfg.programArgs));
    return true;
  }

  /** Stop the live run (if any). */
  async function stop(): Promise<void> {
    const id = runId;
    if (id) {
      try {
        await ipcCancelRun(id);
      } catch {
        /* best-effort — the exit event still flips `running` off */
      }
    }
    running = false;
    runId = null;
    push('Stopped.', 'meta');
  }

  return {
    get building() { return building; },
    get running() { return running; },
    get validating() { return validating; },
    /** Busy = a build, validation or run is in flight (drives disabling the build/▶ buttons). */
    get active() { return building || running || validating; },
    get tool() { return tool; },
    get ok() { return ok; },
    get diagnostics() { return diagnostics; },
    get lines() { return lines; },
    get validationResult() { return validationResult; },
    get validateProgress() { return validateProgress; },
    get preferredBuildType() { return preferredBuildType; },

    /** The remembered main class for `root`, or null. */
    mainClassFor(root: string): string | null {
      return mainClasses.get(root) ?? null;
    },

    attach,
    build,
    validateProject,
    runPreferred,
    setPreferredBuildType,
    run,
    runActive,
    stop,
    /** Clear the log + last result (the Build panel's "clear" action). */
    clear() {
      lines = [];
      diagnostics = [];
      ok = null;
      tool = '';
      validationResult = null;
    },
  };
}

export const bennuRunStore = createBennuRunStore();
