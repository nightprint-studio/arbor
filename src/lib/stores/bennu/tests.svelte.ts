/**
 * Bennu unit-test store — discovery, the live run, and the tree the Tests panel draws.
 *
 * The backend (`bennu-be`, `tests.rs`) spawns `mvn test` and streams four things: raw
 * output, "this class is running now", a full report per class **as it finishes**, and the
 * exit. This store owns the FE side of that: the accumulating results, the log buffer, the
 * scope of the last run (so Rerun needs no arguments), and the view model.
 *
 * ## The one thing worth understanding
 *
 * A **declared** test and an **executed** case are different things, and the tree shows
 * whichever it has. Before a class runs, its rows come from discovery — the methods the
 * source declares, greyed as pending. Once its report lands, the rows are replaced by what
 * actually ran. This is not a cosmetic choice: one `@ParameterizedTest` declaration produces
 * many cases, an inherited test is declared in a base class and reported under the concrete
 * one, and a class that fails to initialise reports a case that was never declared. Matching
 * the two sets up would be guesswork in exactly the cases where being wrong matters, so the
 * store never tries: it swaps one for the other at the moment the truth arrives.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteMap } from 'svelte/reactivity';
import { discoverTests, runTests, cancelTests } from '$lib/ipc/bennu/tests';
import type {
  DiscoveredTest, TestCaseRef, TestCaseResult, TestClassResult, TestRunTotals, TestScope,
} from '$lib/types/bennu';
import { bennuUiStore } from './ui.svelte';
import type { RunLogLine } from './run.svelte';

/** Cap the retained log so a chatty Maven run can't grow the buffer unbounded. What this
 *  bounds is memory: the console renders only what is on screen ({@link BennuConsole}), so the
 *  old, much lower cap was paying for a DOM that no longer exists. */
const MAX_LINES = 10_000;

/** What a row in the tree is showing. `pending` = declared but not run (yet). */
export type RowStatus = 'passed' | 'failed' | 'error' | 'skipped' | 'running' | 'pending';

/** One row of the test tree — a class, or one of its cases. */
export interface TestRow {
  /** Stable key for `{#each}` and for selection. */
  id: string;
  kind: 'class' | 'case';
  /** What the row reads as: the simple class name, or the method name. */
  label: string;
  /** The declaring class, fully qualified with dots. */
  classname: string;
  /**
   * The name to hand Surefire when running this row's class (`OrderTest`,
   * `OuterTest$Inner`). Carried on the row rather than derived from `classname` at the call
   * site: the two spellings differ for a nested class, and deriving one from the other is
   * exactly the step that silently runs the wrong thing.
   */
  selector: string;
  /** For a case row, its class row's id — what ← navigates to. */
  parentId?: string;
  /** For a case row, the method name as the report wrote it. */
  method?: string;
  status: RowStatus;
  /** Duration in ms; `null` for a row that hasn't run. */
  timeMs: number | null;
  flaky: boolean;
  /** Source location, when discovery knows it — this is what makes a row double-clickable. */
  file?: string;
  line?: number;
  offset?: number;
  disabled: boolean;
  disabledReason?: string | null;
  message?: string | null;
  /** The exception type of a failure (`java.lang.AssertionError`). */
  errorKind?: string | null;
  trace?: string | null;
  /** Whatever the class printed, hung off the class row. */
  systemOut?: string | null;
  children: TestRow[];
  /** For a class row: how its cases came out. */
  counts?: { total: number; bad: number; skipped: number };
}

/** Whether a status should read as a failure. */
export function isBad(status: RowStatus): boolean {
  return status === 'failed' || status === 'error';
}

/**
 * The method name to hand back to Surefire, stripped of anything an invocation added.
 *
 * A `@ParameterizedTest` is reported as `converts(int)[1]` or `[1] input=3, expected=6` —
 * shapes that are not method names and that `-Dtest=Class#…` cannot match. Rerunning the
 * declaration re-runs all of its invocations, which is both what Surefire can express and
 * what you want: a parameterized case that failed rarely failed alone.
 */
export function baseMethodName(name: string): string {
  const cut = name.search(/[([\s]/);
  return (cut === -1 ? name : name.slice(0, cut)).trim();
}

/** The Surefire selector name for a reported class (`com.acme.Outer$Inner` → `Outer$Inner`). */
function selectorOf(classname: string): string {
  return classname.split('.').pop() ?? classname;
}

/** A class's overall status from its report. An error outranks a failure — a class that
 *  couldn't run is a bigger fact than one that disagreed. */
function statusOfResult(r: TestClassResult): RowStatus {
  if (r.errors > 0) return 'error';
  if (r.failures > 0) return 'failed';
  if (r.total > 0 && r.skipped === r.total) return 'skipped';
  return 'passed';
}

function createBennuTestStore() {
  // ── discovery ──────────────────────────────────────────────────────────────
  let discovered = $state<DiscoveredTest[]>([]);
  let discovering = $state(false);
  let discoveredRoot = '';

  // ── the live run ───────────────────────────────────────────────────────────
  let running = $state(false);
  let label = $state('');
  let widened = $state<string | null>(null);
  let runningClass = $state<string | null>(null);
  let exitCode = $state<number | null>(null);
  let cancelled = $state(false);
  let totals = $state<TestRunTotals | null>(null);
  let startedAt = $state(0);
  let elapsedMs = $state(0);
  let lines = $state<RunLogLine[]>([]);
  /** Reports as they land, keyed by the class's DOTTED name so they line up with discovery. */
  const results = new SvelteMap<string, TestClassResult>();

  // Not reactive — only the event handlers and `stop()` read it.
  let runId: string | null = null;
  let lastScope: TestScope | null = null;
  let lastRoot = '';
  let ticker: ReturnType<typeof setInterval> | null = null;

  // ── view options ───────────────────────────────────────────────────────────
  let onlyFailed = $state(false);
  let sortByTime = $state(false);
  let selectedId = $state<string | null>(null);

  let attached = false;
  let unlisteners: UnlistenFn[] = [];

  /** Append a line. `log` is the backend's interpretation of it (level + pieces), absent on
   *  the status lines this store writes itself. */
  function push(
    text: string,
    stream: RunLogLine['stream'] = 'out',
    log?: Pick<RunLogLine, 'level' | 'pieces'>,
  ) {
    const next = lines.length >= MAX_LINES ? lines.slice(lines.length - MAX_LINES + 1) : lines.slice();
    next.push({ text, stream, ...log });
    lines = next;
  }

  /** A reported class name, keyed the way discovery spells it (`Outer$Inner` → `Outer.Inner`). */
  function key(classname: string): string {
    return classname.replace(/\$/g, '.');
  }

  function startTicker() {
    stopTicker();
    ticker = setInterval(() => { elapsedMs = Date.now() - startedAt; }, 500);
  }
  function stopTicker() {
    if (ticker !== null) { clearInterval(ticker); ticker = null; }
  }

  /** Attach the test event listeners. Called once from BennuWindow.onMount; returns a
   *  detach fn. Idempotent. */
  async function attach(): Promise<UnlistenFn> {
    if (attached) return detach;
    attached = true;
    const add = (f: UnlistenFn) => unlisteners.push(f);
    const mine = (id: string) => runId === null || id === runId;

    add(await listen<{
      run_id: string;
      stream: string;
      text: string;
      level?: RunLogLine['level'];
      pieces?: RunLogLine['pieces'];
    }>(
      'arbor://bennu/test-output',
      (e) => {
        if (!mine(e.payload.run_id)) return;
        push(e.payload.text, e.payload.stream === 'stderr' ? 'err' : 'out', {
          level: e.payload.level,
          pieces: e.payload.pieces,
        });
      },
    ));
    add(await listen<{ run_id: string; classname: string }>(
      'arbor://bennu/test-running',
      (e) => { if (mine(e.payload.run_id)) runningClass = key(e.payload.classname); },
    ));
    add(await listen<{ run_id: string; result: TestClassResult }>(
      'arbor://bennu/test-class',
      (e) => {
        if (!mine(e.payload.run_id)) return;
        const r = e.payload.result;
        results.set(key(r.classname), r);
        // The class that just reported is no longer the one running.
        if (runningClass === key(r.classname)) runningClass = null;
      },
    ));
    add(await listen<{ run_id: string; code: number | null; cancelled: boolean; totals: TestRunTotals | null }>(
      'arbor://bennu/test-exit',
      (e) => {
        if (!mine(e.payload.run_id)) return;
        running = false;
        runId = null;
        runningClass = null;
        exitCode = e.payload.code;
        cancelled = e.payload.cancelled;
        totals = e.payload.totals;
        elapsedMs = Date.now() - startedAt;
        stopTicker();
        push(
          e.payload.cancelled
            ? 'Stopped.'
            : `Finished in ${formatDuration(elapsedMs)} — ${summaryLine()}`,
          e.payload.cancelled || failedCount() > 0 ? 'err' : 'meta',
        );
      },
    ));
    return detach;
  }
  function detach() {
    for (const f of unlisteners) f();
    unlisteners = [];
    stopTicker();
    attached = false;
  }

  /** Load (or reload) the project's test classes. Cheap after the first call — the backend
   *  caches the walk — so callers may ask freely. */
  async function discover(root: string, force = false): Promise<void> {
    if (!root) return;
    if (!force && root === discoveredRoot && discovered.length > 0) return;
    discovering = true;
    try {
      discovered = await discoverTests(root, { force });
      discoveredRoot = root;
    } catch {
      // A discovery failure is not worth a toast: the panel shows "no tests found", which
      // is the same thing the user needs to know and one fewer interruption.
      discovered = [];
    } finally {
      discovering = false;
    }
  }

  /**
   * Run `scope`. Clears the previous run's results, opens the Tests panel and streams.
   * No-op while a run is in flight — the backend refuses a second Maven on the same tree
   * anyway, and asking only to be told no is worse than not asking.
   */
  async function run(root: string, scope: TestScope): Promise<void> {
    if (running || !root) return;
    running = true;
    lastScope = scope;
    lastRoot = root;
    results.clear();
    lines = [];
    runningClass = null;
    exitCode = null;
    cancelled = false;
    totals = null;
    widened = null;
    selectedId = null;
    startedAt = Date.now();
    elapsedMs = 0;
    startTicker();
    // The Run console, on its Tests tab — starting a run brings the thing you started forward.
    bennuUiStore.showTestRun();
    try {
      const handle = await runTests(root, scope);
      runId = handle.run_id;
      label = handle.label;
      widened = handle.widened;
      push(`Running ${handle.label}…`, 'meta');
      if (handle.widened) push(handle.widened, 'err');
    } catch (e) {
      running = false;
      runId = null;
      stopTicker();
      push(`Could not start the tests: ${e instanceof Error ? e.message : String(e)}`, 'err');
    }
  }

  /** Stop the live run — the backend kills Maven and everything it started. */
  async function stop(): Promise<void> {
    const id = runId;
    if (!id) return;
    try {
      await cancelTests(id);
    } catch {
      /* best-effort — the exit event still flips `running` off */
    }
  }

  /** Every case that failed or errored, as a scope — the Rerun-failed action. */
  function failedCases(): TestCaseRef[] {
    const out: TestCaseRef[] = [];
    const seen = new Set<string>();
    for (const r of results.values()) {
      for (const c of r.cases) {
        if (c.status !== 'failed' && c.status !== 'error') continue;
        const ref = { class: selectorOf(c.classname || r.classname), method: baseMethodName(c.name) };
        const k = `${ref.class}#${ref.method}`;
        if (seen.has(k)) continue;
        seen.add(k);
        out.push(ref);
      }
    }
    return out;
  }

  function failedCount(): number {
    let n = 0;
    for (const r of results.values()) n += r.failures + r.errors;
    return n;
  }

  function summaryLine(): string {
    const c = counts();
    const bits = [`${c.passed} passed`];
    if (c.failed) bits.push(`${c.failed} failed`);
    if (c.errored) bits.push(`${c.errored} errored`);
    if (c.skipped) bits.push(`${c.skipped} skipped`);
    return bits.join(', ');
  }

  /** Totals across every report received so far. */
  function counts() {
    let passed = 0, failed = 0, errored = 0, skipped = 0;
    for (const r of results.values()) {
      failed += r.failures;
      errored += r.errors;
      skipped += r.skipped;
      passed += r.total - r.failures - r.errors - r.skipped;
    }
    return { passed, failed, errored, skipped, total: passed + failed + errored + skipped };
  }

  // ── the tree ───────────────────────────────────────────────────────────────

  /** Class rows the user has folded shut. Only classes fold — a case has no children. */
  const collapsed = new SvelteMap<string, true>();

  /** Case rows for a class that has reported. `parentId` is the class row's key, which is
   *  the dotted spelling — the report's `$` form would not match it. */
  function caseRows(
    r: TestClassResult,
    decl: DiscoveredTest | undefined,
    parentId: string,
  ): TestRow[] {
    return r.cases.map((c: TestCaseResult, i) => {
      // A declared method of the same name gives the row a source location, so it can be
      // opened. A parameterized invocation won't match — it keeps the class's location.
      const m = decl?.methods.find((d) => d.name === baseMethodName(c.name));
      return {
        id: `${r.classname}#${c.name}#${i}`,
        kind: 'case' as const,
        label: c.name,
        classname: c.classname || r.classname,
        // The report spells a nested class `Outer$Inner`, which IS the selector.
        selector: selectorOf(c.classname || r.classname),
        parentId,
        method: c.name,
        status: c.status as RowStatus,
        timeMs: c.time_ms,
        flaky: c.flaky,
        file: decl?.file,
        line: m?.line ?? decl?.line,
        offset: m?.offset ?? decl?.offset,
        disabled: false,
        message: c.message,
        errorKind: c.kind,
        trace: c.trace,
        children: [],
      };
    });
  }

  /** Case rows for a class that hasn't run: what the source declares. */
  function declaredRows(d: DiscoveredTest, status: RowStatus): TestRow[] {
    return d.methods.map((m) => ({
      id: `${d.fqcn}#${m.name}`,
      kind: 'case' as const,
      label: m.name,
      classname: d.fqcn,
      selector: d.selector,
      parentId: d.fqcn,
      method: m.name,
      status: m.disabled ? ('skipped' as RowStatus) : status,
      timeMs: null,
      flaky: false,
      file: d.file,
      line: m.line,
      offset: m.offset,
      disabled: m.disabled,
      disabledReason: m.disabled_reason,
      children: [],
    }));
  }

  /**
   * The tree the panel draws: every class that ran, plus — while idle — every class the
   * project declares, so the panel is a place to launch from and not only a place to read
   * results in.
   */
  const rows = $derived.by<TestRow[]>(() => {
    const byName = new Map<string, DiscoveredTest>();
    for (const d of discovered) byName.set(d.fqcn, d);

    // While a run is on, show only what that run touches; idle, show everything declared.
    const names = new Set<string>(results.keys());
    if (!running && results.size === 0) for (const n of byName.keys()) names.add(n);
    if (runningClass) names.add(runningClass);

    let out: TestRow[] = [];
    for (const name of names) {
      const decl = byName.get(name);
      const result = results.get(name);
      const status: RowStatus = result
        ? statusOfResult(result)
        : runningClass === name
          ? 'running'
          : 'pending';
      const children = result
        ? caseRows(result, decl, name)
        : decl
          ? declaredRows(decl, status)
          : [];
      out.push({
        id: name,
        kind: 'class',
        label: decl ? decl.fqcn.slice(decl.package ? decl.package.length + 1 : 0) : shortName(name),
        classname: name,
        // Discovery knows the exact spelling; without it, the last dotted segment is the
        // simple name and — because a report writes nested classes with `$` — still correct.
        selector: decl?.selector ?? selectorOf(name),
        status,
        timeMs: result ? result.time_ms : null,
        flaky: children.some((c) => c.flaky),
        file: decl?.file,
        line: decl?.line,
        offset: decl?.offset,
        disabled: decl?.disabled ?? false,
        systemOut: result?.system_out ?? null,
        children,
        counts: result
          ? {
              total: result.total,
              bad: result.failures + result.errors,
              skipped: result.skipped,
            }
          : undefined,
      });
    }

    if (onlyFailed) {
      out = out
        .filter((c) => isBad(c.status))
        .map((c) => ({ ...c, children: c.children.filter((x) => isBad(x.status)) }));
    }

    const byLabel = (a: TestRow, b: TestRow) => a.label.localeCompare(b.label);
    const byTime = (a: TestRow, b: TestRow) => (b.timeMs ?? -1) - (a.timeMs ?? -1);
    const cmp = sortByTime ? byTime : byLabel;
    out.sort(cmp);
    for (const c of out) c.children = [...c.children].sort(sortByTime ? byTime : byLabel);
    return out;
  });

  /** Flattened rows, respecting collapse — the panel's keyboard navigation walks this. */
  const flatRows = $derived.by<TestRow[]>(() => {
    const out: TestRow[] = [];
    for (const c of rows) {
      out.push(c);
      if (!collapsed.has(c.id)) out.push(...c.children);
    }
    return out;
  });

  return {
    get discovered() { return discovered; },
    get discovering() { return discovering; },
    get running() { return running; },
    get label() { return label; },
    get widened() { return widened; },
    get runningClass() { return runningClass; },
    get exitCode() { return exitCode; },
    get cancelled() { return cancelled; },
    get mavenTotals() { return totals; },
    get elapsedMs() { return elapsedMs; },
    get lines() { return lines; },
    get rows() { return rows; },
    get flatRows() { return flatRows; },
    get counts() { return counts(); },
    /** Whether anything has been run — drives the panel's empty state. */
    get hasResults() { return results.size > 0; },
    get onlyFailed() { return onlyFailed; },
    get sortByTime() { return sortByTime; },
    get selectedId() { return selectedId; },
    /** The row the output pane is showing. */
    get selected(): TestRow | null {
      return flatRows.find((r) => r.id === selectedId) ?? null;
    },
    /** Whether "rerun failed" has anything to do. */
    get hasFailures() { return failedCount() > 0; },

    isCollapsed(id: string) { return collapsed.has(id); },
    toggleCollapsed(id: string) {
      if (collapsed.has(id)) collapsed.delete(id);
      else collapsed.set(id, true);
    },
    expandAll() { collapsed.clear(); },
    collapseAll() { for (const c of rows) collapsed.set(c.id, true); },

    select(id: string | null) { selectedId = id; },
    setOnlyFailed(v: boolean) { onlyFailed = v; },
    setSortByTime(v: boolean) { sortByTime = v; },

    attach,
    discover,
    run,
    stop,

    /** Every test in the project. */
    runAll(root: string) { return run(root, { kind: 'all' }); },
    /** Every test in one class (by its Surefire selector name). */
    runClass(root: string, selector: string) {
      return run(root, { kind: 'classes', classes: [selector] });
    },
    /** One method. */
    runCase(root: string, selector: string, method: string) {
      return run(root, { kind: 'cases', cases: [{ class: selector, method }] });
    },
    /** A set of classes — how a package, a folder or a multi-selection arrives. */
    runClasses(root: string, selectors: string[]) {
      return run(root, { kind: 'classes', classes: selectors });
    },
    /** Every test in one Maven module. */
    runModule(root: string, module: string) {
      return run(root, { kind: 'module', module });
    },

    /** Re-run exactly what was run last. */
    rerun() {
      if (!lastScope || !lastRoot) return Promise.resolve();
      return run(lastRoot, lastScope);
    },
    /** Re-run only the cases that failed or errored. */
    rerunFailed() {
      const cases = failedCases();
      if (!cases.length || !lastRoot) return Promise.resolve();
      return run(lastRoot, { kind: 'cases', cases });
    },

    /** The test classes declared in `file` — what "run the test at the caret" resolves in. */
    classesInFile(file: string): DiscoveredTest[] {
      const norm = file.replace(/\\/g, '/');
      return discovered.filter((d) => d.file === norm);
    },
    /** The test classes under a directory — the tree's "Run tests in …". */
    classesUnder(dir: string): DiscoveredTest[] {
      const norm = dir.replace(/\\/g, '/').replace(/\/$/, '');
      return discovered.filter((d) => d.file.startsWith(`${norm}/`) && !d.is_abstract);
    },

    /** Clear the last run's results + log (the panel's Clear action). */
    clear() {
      results.clear();
      lines = [];
      exitCode = null;
      cancelled = false;
      totals = null;
      widened = null;
      selectedId = null;
      elapsedMs = 0;
    },
    /** Forget everything — called when the project changes. */
    reset() {
      discovered = [];
      discoveredRoot = '';
      results.clear();
      lines = [];
      collapsed.clear();
      running = false;
      runId = null;
      lastScope = null;
      lastRoot = '';
      runningClass = null;
      exitCode = null;
      cancelled = false;
      totals = null;
      widened = null;
      selectedId = null;
      elapsedMs = 0;
      stopTicker();
    },
  };
}

/** `1.2s` / `340ms` / `1m 05s` — the same reading the Build panel uses. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  return `${m}m ${String(Math.round(s % 60)).padStart(2, '0')}s`;
}

/** The last segment of a dotted name. */
function shortName(fqcn: string): string {
  return fqcn.split('.').pop() ?? fqcn;
}

export const bennuTestStore = createBennuTestStore();
