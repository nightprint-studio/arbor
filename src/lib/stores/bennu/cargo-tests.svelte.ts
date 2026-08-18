/**
 * Cargo test store — discovery, the live run, and the tree the Tests panel draws for a Rust
 * workspace.
 *
 * The Maven store's sibling ({@link bennuTestStore}), and deliberately not a branch inside it: what
 * a test *is* differs at the root. Maven reports a class at a time and its tree is two levels;
 * cargo reports a case at a time and its tree is **four** — crate → target → module → test.
 *
 * ## Why four levels and not one flat list
 *
 * A `tests::works` row is meaningless in a twenty-crate workspace: there are twenty of them. The
 * grouping is not decoration, it is the only thing that makes a row identifiable, and it is also
 * exactly what a run can be narrowed to — every level of the tree is a scope
 * ({@link CargoTestScope}) the ▷ button on that row can hand to cargo.
 *
 * ## Declared rows and reported rows live together
 *
 * The Maven store swaps one set for the other when a class's report lands, because Surefire says
 * nothing until then. libtest reports **as each case finishes**, so this store does better: the
 * declared rows are there from the start and each one turns green, red or grey as its result
 * arrives. Two consequences worth knowing:
 *
 * - A reported case that no declaration matches is **appended**, not dropped. That is how an
 *   `#[rstest]`'s real cases (`adds::case_1`, `adds::case_2`) show up, and how a test the scan
 *   missed still lands in the panel.
 * - A declaration whose cases arrived under it (`adds` with `adds::case_1` reported) is **hidden**,
 *   or the panel would show a parent that never runs beside the children that did.
 *
 * ## What cargo cannot tell us
 *
 * libtest prints no per-case duration without an unstable flag, so a case row carries no time. The
 * per-binary `finished in` is real and sits on the target row. Inventing a per-case number by
 * dividing would be a fabrication the panel would then sort by.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SvelteMap } from 'svelte/reactivity';
import { discoverCargoTests, runCargoTests } from '$lib/ipc/bennu/cargo-tests';
import { cancelTests } from '$lib/ipc/bennu/tests';
import type {
  CargoCaseEvent, CargoCompilingEvent, CargoTargetDoneEvent, CargoTargetEvent, CargoTestScope,
  CargoTestTarget, DiscoveredRustTest, RustCaseRef, TestRunTotals,
} from '$lib/types/bennu';
import { formatDuration, isBad, type RowStatus, type TestRow } from './test-tree';
import { bennuUiStore } from './ui.svelte';
import type { RunLogLine } from './run.svelte';

/** Cap the retained log so a chatty run cannot grow the buffer unbounded. */
const MAX_LINES = 10_000;

/**
 * How long results are gathered before the tree is rebuilt, in ms.
 *
 * libtest prints a line **per test as it finishes**, so a 2 000-test workspace delivers 2 000
 * events over a few seconds — and every one of them invalidates the tree. Rebuilding 2 000 rows
 * two hundred times a second is how a WebView stops answering. Batching them costs an eighth of a
 * second of latency on a row turning green, which nobody can see, and turns the rebuild count from
 * "one per test" into "eight per second".
 *
 * A target finishing and the run exiting both flush immediately, so the final state is never late.
 */
const FLUSH_MS = 120;

/** A run this store owns. The exit topic is shared with the Maven runner, and the backend prefixes
 *  cargo run ids so each store can recognise its own without a second topic. */
function isCargoRun(id: string): boolean {
  return id.startsWith('cargo-');
}

/** A target's id — `lib`, `bin:cli`, `test:api`. The tree's node key, and stable across runs. */
export function targetId(t: CargoTestTarget | null | undefined): string {
  if (!t) return 'unknown';
  return 'name' in t ? `${t.kind}:${t.name}` : t.kind;
}

/** How a target reads in the tree. */
export function targetLabel(t: CargoTestTarget | null | undefined, fallback = 'tests'): string {
  if (!t) return fallback;
  switch (t.kind) {
    case 'lib': return 'lib';
    case 'doc': return 'doc-tests';
    default: return `${t.kind} ${t.name}`;
  }
}

/** One case as the run reported it. */
interface CaseResult {
  package: string;
  target: CargoTestTarget | null;
  module: string;
  name: string;
  path: string;
  status: RowStatus;
  note: string | null;
  message: string | null;
}

/** One target's block, as it started and as it ended. */
interface TargetBlock {
  index: number;
  package: string;
  target: CargoTestTarget | null;
  desc: string;
  count: number;
  timeMs: number | null;
  /** Set once libtest's summary for the block has arrived. */
  done: boolean;
}

function createCargoTestStore() {
  // ── discovery ────────────────────────────────────────────────────────────────
  let discovered = $state<DiscoveredRustTest[]>([]);
  let discovering = $state(false);
  let discoveredRoot = '';

  // ── the live run ─────────────────────────────────────────────────────────────
  let running = $state(false);
  let label = $state('');
  let command = $state('');
  let widened = $state<string | null>(null);
  let compiling = $state<string | null>(null);
  let exitCode = $state<number | null>(null);
  let cancelled = $state(false);
  let totals = $state<TestRunTotals | null>(null);
  let startedAt = $state(0);
  let elapsedMs = $state(0);
  let lines = $state<RunLogLine[]>([]);
  /** Cases as they land, keyed by `<package>|<targetId>|<path>` — the identity a declared row has
   *  too, which is what lets the two be matched without guessing. */
  const results = new SvelteMap<string, CaseResult>();
  /** Failure output arrives after the verdict, keyed by path alone (libtest's block header carries
   *  nothing else). Kept apart so a message can never overwrite a status. */
  const messages = new SvelteMap<string, string>();
  /** Target blocks by index, in the order cargo started them. */
  const blocks = new SvelteMap<number, TargetBlock>();
  /** Results that have arrived but not yet been published to the reactive maps above — see
   *  {@link FLUSH_MS}. Plain Maps on purpose: writing to these must invalidate nothing. */
  const pendingCases = new Map<string, CaseResult>();
  const pendingMessages = new Map<string, string>();
  let flushTimer: ReturnType<typeof setTimeout> | null = null;

  // Not reactive — only the event handlers and `stop()` read these.
  let runId: string | null = null;
  let lastScope: CargoTestScope | null = null;
  let lastRoot = '';
  let includeIgnored = false;
  let ticker: ReturnType<typeof setInterval> | null = null;

  // ── view options ─────────────────────────────────────────────────────────────
  let onlyFailed = $state(false);
  let sortByTime = $state(false);
  let selectedId = $state<string | null>(null);
  const collapsed = new SvelteMap<string, true>();

  let attached = false;
  let unlisteners: UnlistenFn[] = [];

  function push(
    text: string,
    stream: RunLogLine['stream'] = 'out',
    log?: Pick<RunLogLine, 'level' | 'pieces'>,
  ) {
    const next =
      lines.length >= MAX_LINES ? lines.slice(lines.length - MAX_LINES + 1) : lines.slice();
    next.push({ text, stream, ...log });
    lines = next;
  }

  /** Publish everything buffered, and stand the timer down. */
  function flushPending() {
    if (flushTimer !== null) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    if (pendingCases.size === 0 && pendingMessages.size === 0) return;
    for (const [k, v] of pendingCases) results.set(k, v);
    pendingCases.clear();
    for (const [k, v] of pendingMessages) messages.set(k, v);
    pendingMessages.clear();
  }

  function schedulePublish() {
    if (flushTimer === null) flushTimer = setTimeout(flushPending, FLUSH_MS);
  }

  function startTicker() {
    stopTicker();
    ticker = setInterval(() => { elapsedMs = Date.now() - startedAt; }, 500);
  }
  function stopTicker() {
    if (ticker !== null) { clearInterval(ticker); ticker = null; }
  }

  function caseKey(pkg: string, t: CargoTestTarget | null | undefined, path: string): string {
    return `${pkg}|${targetId(t)}|${path}`;
  }

  /** Attach the event listeners. Called once from BennuWindow.onMount; returns a detach fn. */
  async function attach(): Promise<UnlistenFn> {
    if (attached) return detach;
    attached = true;
    const add = (f: UnlistenFn) => unlisteners.push(f);
    // Strict on the prefix, tolerant on the id: an event can arrive before the handle's `await`
    // has resolved, so `runId` may still be null for the first few.
    const mine = (id: string) => isCargoRun(id) && (runId === null || id === runId);

    add(await listen<{
      run_id: string;
      stream: string;
      text: string;
      level?: RunLogLine['level'];
      pieces?: RunLogLine['pieces'];
    }>('arbor://bennu/test-output', (e) => {
      if (!mine(e.payload.run_id)) return;
      push(e.payload.text, e.payload.stream === 'stderr' ? 'err' : 'out', {
        level: e.payload.level,
        pieces: e.payload.pieces,
      });
    }));

    add(await listen<CargoCompilingEvent>('arbor://bennu/cargo-test-compiling', (e) => {
      if (mine(e.payload.run_id)) compiling = e.payload.crate;
    }));

    add(await listen<CargoTargetEvent>('arbor://bennu/cargo-test-target', (e) => {
      if (!mine(e.payload.run_id)) return;
      compiling = null;
      const p = e.payload;
      blocks.set(p.index, {
        index: p.index,
        package: p.package,
        target: p.target,
        desc: p.desc,
        count: p.count,
        timeMs: null,
        done: false,
      });
    }));

    add(await listen<CargoCaseEvent>('arbor://bennu/cargo-test-case', (e) => {
      if (!mine(e.payload.run_id)) return;
      const p = e.payload;
      // No status → this event amends a row that a verdict already created.
      if (!p.status) {
        if (p.message) {
          pendingMessages.set(p.path, p.message);
          schedulePublish();
        }
        return;
      }
      const key = caseKey(p.package ?? '', p.target, p.path);
      pendingCases.set(key, {
        package: p.package ?? '',
        target: p.target ?? null,
        module: p.module ?? '',
        name: p.name ?? p.path,
        path: p.path,
        status: p.status,
        note: p.note ?? null,
        message: null,
      });
      schedulePublish();
    }));

    add(await listen<CargoTargetDoneEvent>('arbor://bennu/cargo-test-target-done', (e) => {
      if (!mine(e.payload.run_id)) return;
      // A target's summary is the moment its rows are final, so publish before recording it —
      // otherwise the counts on the target row would be one flush behind its own verdict.
      flushPending();
      const b = blocks.get(e.payload.index);
      if (b) blocks.set(e.payload.index, { ...b, timeMs: e.payload.result.time_ms, done: true });
    }));

    add(await listen<{
      run_id: string; code: number | null; cancelled: boolean; totals: TestRunTotals | null;
    }>('arbor://bennu/test-exit', (e) => {
      if (!mine(e.payload.run_id)) return;
      flushPending();
      running = false;
      runId = null;
      compiling = null;
      exitCode = e.payload.code;
      cancelled = e.payload.cancelled;
      totals = e.payload.totals;
      elapsedMs = Date.now() - startedAt;
      stopTicker();
      push(
        e.payload.cancelled
          ? 'Stopped.'
          : `Finished in ${formatDuration(elapsedMs)} — ${summaryLine()}`,
        e.payload.cancelled || counts().failed > 0 ? 'err' : 'meta',
      );
    }));
    return detach;
  }

  function detach() {
    for (const f of unlisteners) f();
    unlisteners = [];
    stopTicker();
    attached = false;
  }

  /** Load (or reload) the workspace's tests. Cheap after the first call — the backend caches the
   *  walk — so callers may ask freely. */
  async function discover(root: string, force = false): Promise<void> {
    if (!root) return;
    if (!force && root === discoveredRoot && discovered.length > 0) return;
    discovering = true;
    try {
      discovered = await discoverCargoTests(root, { force });
      discoveredRoot = root;
    } catch {
      // A workspace with no manifest is not an error worth a toast — the panel's empty state
      // already says there is nothing to run.
      discovered = [];
    } finally {
      discovering = false;
    }
  }

  async function run(root: string, scope: CargoTestScope): Promise<void> {
    if (!root || running) return;
    flushPending();
    pendingCases.clear();
    pendingMessages.clear();
    results.clear();
    messages.clear();
    blocks.clear();
    lines = [];
    exitCode = null;
    cancelled = false;
    totals = null;
    widened = null;
    selectedId = null;
    compiling = null;
    lastScope = scope;
    lastRoot = root;
    running = true;
    startedAt = Date.now();
    elapsedMs = 0;
    startTicker();
    // The panel is where the run is watched, so put it in front of the user rather than making
    // them go and find it.
    bennuUiStore.showTestRun();
    try {
      const handle = await runCargoTests(root, scope, includeIgnored);
      runId = handle.run_id;
      label = handle.label;
      command = handle.command;
      widened = handle.widened;
      push(`> ${handle.command}`, 'meta');
      if (handle.widened) push(handle.widened, 'err');
    } catch (e) {
      running = false;
      stopTicker();
      push(String(e instanceof Error ? e.message : e), 'err');
    }
  }

  async function stop(): Promise<void> {
    if (!runId) return;
    try {
      await cancelTests(runId);
    } catch {
      // The run had already finished; the exit event will (or did) tidy up.
    }
  }

  // ── the tree ─────────────────────────────────────────────────────────────────

  /** Every reported case, in arrival order. */
  const reported = $derived.by(() => [...results.values()]);

  /** The cases that failed — what Rerun-failed hands back. */
  function failedCases(): RustCaseRef[] {
    const out: RustCaseRef[] = [];
    for (const c of reported) {
      if (!isBad(c.status) || !c.target) continue;
      // A parameterized case (`adds::case_1`) is rerun by its declaration's prefix: libtest can
      // name the generated case exactly, but the declaration is what the panel and the source
      // agree on, and rerunning one generated case rarely means rerunning only that one.
      const decl = declarationOf(c);
      out.push(
        decl
          ? { package: decl.package, target: decl.target, path: decl.path, exact: decl.kind !== 'parameterized' }
          : { package: c.package, target: c.target, path: c.path, exact: true },
      );
    }
    // Same declaration reached twice (two generated cases of one `#[rstest]`) is one filter. By
    // Set rather than by `findIndex`, which is quadratic — and "rerun failed" on a workspace where
    // a whole crate went red is exactly the case with the most rows.
    const seen = new Set<string>();
    return out.filter((c) => {
      const k = `${c.package}|${c.path}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
  }

  /** Every declaration by `<package>|<targetId>|<path>` — the identity a reported case shares. */
  const declByKey = $derived.by(() => {
    const m = new Map<string, DiscoveredRustTest>();
    for (const d of discovered) m.set(caseKey(d.package, d.target, d.path), d);
    return m;
  });

  /**
   * The declaration a reported case came from, when discovery knows it.
   *
   * An exact hit first; failing that, the path's ancestors, because a generated case
   * (`adds::case_1`) is declared as `adds`. Walking the ancestors is bounded by how deep the
   * module path is — a scan of every declaration would be linear in the *test count*, and this is
   * called once per reported case, which is how a 2 000-test run became quadratic.
   */
  function declarationOf(c: CaseResult): DiscoveredRustTest | undefined {
    const exact = declByKey.get(caseKey(c.package, c.target, c.path));
    if (exact) return exact;
    let at = c.path.lastIndexOf('::');
    while (at > 0) {
      const hit = declByKey.get(caseKey(c.package, c.target, c.path.slice(0, at)));
      if (hit) return hit;
      at = c.path.lastIndexOf('::', at - 1);
    }
    return undefined;
  }

  function counts() {
    let passed = 0;
    let failed = 0;
    let skipped = 0;
    for (const c of reported) {
      if (c.status === 'skipped') skipped += 1;
      else if (isBad(c.status)) failed += 1;
      else passed += 1;
    }
    // `errored` exists for the Maven side, which distinguishes "threw" from "disagreed". libtest
    // does not, so it is always 0 here rather than absent — the panel adds the two together.
    return { total: passed + failed + skipped, passed, failed, errored: 0, skipped };
  }

  function summaryLine(): string {
    const c = counts();
    if (c.total === 0) return 'no tests ran';
    const bits = [`${c.passed} passed`];
    if (c.failed) bits.push(`${c.failed} failed`);
    if (c.skipped) bits.push(`${c.skipped} ignored`);
    return bits.join(', ');
  }

  /** The worst status among `kids` — a group is as bad as its worst row. */
  function worst(kids: TestRow[]): RowStatus {
    if (kids.some((k) => k.status === 'error')) return 'error';
    if (kids.some((k) => k.status === 'failed')) return 'failed';
    if (kids.some((k) => k.status === 'running')) return 'running';
    if (kids.some((k) => k.status === 'passed')) return 'passed';
    if (kids.length && kids.every((k) => k.status === 'skipped')) return 'skipped';
    return 'pending';
  }

  function rollup(kids: TestRow[]) {
    let total = 0;
    let bad = 0;
    let skipped = 0;
    for (const k of kids) {
      if (k.counts) {
        total += k.counts.total;
        bad += k.counts.bad;
        skipped += k.counts.skipped;
      } else {
        total += 1;
        if (isBad(k.status)) bad += 1;
        if (k.status === 'skipped') skipped += 1;
      }
    }
    return { total, bad, skipped };
  }

  /** A tag for a case row: what makes it unusual, not what it has in common with the rest. */
  function tagOf(d: DiscoveredRustTest | undefined, note: string | null): string | null {
    if (note) return note;
    if (!d) return null;
    if (d.kind === 'async') return 'async';
    if (d.kind === 'bench') return 'bench';
    if (d.kind === 'parameterized') return 'cases';
    if (d.should_panic) return 'should panic';
    return null;
  }

  /**
   * The tree: crate → target → module → test.
   *
   * Built from discovery **and** results at once, so a run fills a tree that was already there.
   * The module level is skipped when a test sits at a crate root, which is the common shape for an
   * integration test and would otherwise cost a row that says nothing.
   */
  const rows = $derived.by<TestRow[]>(() => {
    // ── one pass to index, so the build is linear ─────────────────────────────
    //
    // This used to be nested `filter`/`some`/`find` calls, which on a 2 000-test workspace is
    // 2 000 × 2 000 comparisons per rebuild — and the rebuild happens on every batch of results.
    // That is a frozen WebView, not a slow one. Everything the loop below needs is therefore
    // bucketed by cell first, and every membership question is a Set lookup.
    type Cell = { package: string; target: CargoTestTarget | null; tid: string };
    const cells = new Map<string, Cell>();
    const cellKey = (pkg: string, t: CargoTestTarget | null) => `${pkg}|${targetId(t)}`;
    const declaredIn = new Map<string, DiscoveredRustTest[]>();
    const reportedIn = new Map<string, CaseResult[]>();
    const reportedPathsIn = new Map<string, Map<string, CaseResult>>();
    /** Per cell: the declaration paths that have generated cases under them (`adds` when
     *  `adds::case_1` was reported), so hiding the parent is one lookup rather than a scan. */
    const generatedUnder = new Map<string, Set<string>>();

    const bucket = <T>(m: Map<string, T[]>, key: string, v: T) => {
      const list = m.get(key);
      if (list) list.push(v);
      else m.set(key, [v]);
    };

    for (const d of discovered) {
      const key = cellKey(d.package, d.target);
      cells.set(key, { package: d.package, target: d.target, tid: targetId(d.target) });
      bucket(declaredIn, key, d);
    }
    for (const b of blocks.values()) {
      // A block with no case and no declaration is a binary that ran nothing; showing it is how
      // "the filter matched nothing here" stays visible instead of looking like a pass.
      cells.set(cellKey(b.package, b.target), { package: b.package, target: b.target, tid: targetId(b.target) });
    }
    for (const c of reported) {
      const key = cellKey(c.package, c.target);
      bucket(reportedIn, key, c);
      const byPath = reportedPathsIn.get(key) ?? new Map<string, CaseResult>();
      byPath.set(c.path, c);
      reportedPathsIn.set(key, byPath);
      // Every ancestor of a reported path is a declaration that may have generated it.
      let at = c.path.lastIndexOf('::');
      const prefixes = generatedUnder.get(key) ?? new Set<string>();
      while (at > 0) {
        prefixes.add(c.path.slice(0, at));
        at = c.path.lastIndexOf('::', at - 1);
      }
      generatedUnder.set(key, prefixes);
    }
    const blockOf = new Map<string, TargetBlock>();
    for (const b of blocks.values()) blockOf.set(cellKey(b.package, b.target), b);

    const crates = new Map<string, TestRow>();
    for (const cell of [...cells.values()].sort(
      (a, b) => a.package.localeCompare(b.package) || a.tid.localeCompare(b.tid),
    )) {
      const key = cellKey(cell.package, cell.target);
      const block = blockOf.get(key);
      const targetRowId = `t:${cell.package}|${cell.tid}`;

      // ── the cases of this target: declared, then anything reported that no declaration owns ──
      const declared = declaredIn.get(key) ?? [];
      const mine = reportedIn.get(key) ?? [];
      const minePaths = reportedPathsIn.get(key) ?? new Map<string, CaseResult>();
      const generated = generatedUnder.get(key) ?? new Set<string>();
      const declaredPaths = new Set(declared.map((d) => d.path));
      const caseRows: TestRow[] = [];
      for (const d of declared) {
        // A declaration whose generated cases have arrived is replaced by them.
        if (generated.has(d.path)) continue;
        const hit = minePaths.get(d.path);
        const status: RowStatus = hit
          ? hit.status
          : block && !block.done && running
            ? 'running'
            : d.ignored && block?.done
              ? 'skipped'
              : 'pending';
        caseRows.push({
          id: `c:${cell.package}|${cell.tid}|${d.path}`,
          kind: 'case',
          depth: 0,
          label: d.name,
          classname: `${cell.package} · ${d.path}`,
          method: d.name,
          status,
          timeMs: null,
          flaky: false,
          file: d.file,
          line: d.line,
          offset: d.offset,
          disabled: d.ignored,
          disabledReason: d.ignored ? 'Marked #[ignore]' : null,
          message: messages.get(d.path) ?? hit?.message ?? null,
          tag: tagOf(d, hit?.note ?? null),
          children: [],
        });
      }
      for (const c of mine) {
        if (declaredPaths.has(c.path)) continue;
        const decl = declarationOf(c);
        caseRows.push({
          id: `c:${cell.package}|${cell.tid}|${c.path}`,
          kind: 'case',
          depth: 0,
          label: c.name || c.path,
          classname: `${cell.package} · ${c.path}`,
          method: c.name,
          status: c.status,
          timeMs: null,
          flaky: false,
          file: decl?.file,
          line: decl?.line,
          offset: decl?.offset,
          disabled: false,
          message: messages.get(c.path) ?? null,
          tag: tagOf(decl, c.note),
          children: [],
        });
      }

      // ── group them by module ────────────────────────────────────────────────
      const byModule = new Map<string, TestRow[]>();
      for (const r of caseRows) {
        const path = r.id.slice(r.id.lastIndexOf('|') + 1);
        const mod = path.includes('::') ? path.slice(0, path.lastIndexOf('::')) : '';
        const list = byModule.get(mod) ?? [];
        list.push(r);
        byModule.set(mod, list);
      }
      const targetKids: TestRow[] = [];
      for (const [mod, kids] of [...byModule.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
        kids.sort((a, b) => a.label.localeCompare(b.label));
        if (!mod) {
          // A crate root: the tests hang straight off the target.
          targetKids.push(...kids);
          continue;
        }
        targetKids.push({
          id: `m:${cell.package}|${cell.tid}|${mod}`,
          kind: 'module',
          depth: 0,
          label: mod,
          classname: `${cell.package} · ${mod}`,
          status: worst(kids),
          timeMs: null,
          flaky: false,
          disabled: false,
          children: kids,
          counts: rollup(kids),
        });
      }
      targetKids.sort((a, b) => a.label.localeCompare(b.label));

      const targetRow: TestRow = {
        id: targetRowId,
        kind: 'target',
        depth: 0,
        label: targetLabel(cell.target, block?.desc ?? 'tests'),
        classname: `${cell.package} · ${targetLabel(cell.target)}`,
        status: worst(targetKids),
        timeMs: block?.timeMs ?? null,
        flaky: false,
        disabled: false,
        children: targetKids,
        counts: rollup(targetKids),
      };

      const crate = crates.get(cell.package) ?? {
        id: `p:${cell.package}`,
        kind: 'crate' as const,
        depth: 0,
        label: cell.package || '(unknown crate)',
        classname: cell.package,
        status: 'pending' as RowStatus,
        timeMs: null,
        flaky: false,
        disabled: false,
        children: [],
      };
      crate.children.push(targetRow);
      crates.set(cell.package, crate);
    }

    let out = [...crates.values()];
    for (const c of out) {
      c.status = worst(c.children);
      c.counts = rollup(c.children);
      c.timeMs = c.children.reduce<number | null>(
        (sum, t) => (t.timeMs === null ? sum : (sum ?? 0) + t.timeMs),
        null,
      );
    }
    if (onlyFailed) out = pruneToFailed(out);
    if (sortByTime) sortByTimeDesc(out);
    stampDepth(out, 0);
    return out;
  });

  /** Keep only the rows that lead to a failure. A group with nothing bad under it goes. */
  function pruneToFailed(list: TestRow[]): TestRow[] {
    const out: TestRow[] = [];
    for (const r of list) {
      if (r.children.length === 0) {
        if (isBad(r.status)) out.push(r);
        continue;
      }
      const kids = pruneToFailed(r.children);
      if (kids.length) out.push({ ...r, children: kids, counts: rollup(kids) });
    }
    return out;
  }

  /** Slowest first, at every level. Cases carry no time (see the module doc), so in practice this
   *  reorders crates and targets and leaves their tests alphabetical. */
  function sortByTimeDesc(list: TestRow[]) {
    list.sort((a, b) => (b.timeMs ?? -1) - (a.timeMs ?? -1));
    for (const r of list) sortByTimeDesc(r.children);
  }

  /** Write each row's indentation level. Done once at the end rather than threaded through the
   *  build, because pruning and skipping the module level both change it. */
  function stampDepth(list: TestRow[], depth: number) {
    for (const r of list) {
      r.depth = depth;
      for (const k of r.children) k.parentId = r.id;
      stampDepth(r.children, depth + 1);
    }
  }

  /** Flattened rows, respecting collapse — the panel's keyboard navigation walks this. */
  const flatRows = $derived.by<TestRow[]>(() => {
    const out: TestRow[] = [];
    const walk = (list: TestRow[]) => {
      for (const r of list) {
        out.push(r);
        if (r.children.length && !collapsed.has(r.id)) walk(r.children);
      }
    };
    walk(rows);
    return out;
  });

  /** The scope that runs what a row stands for. */
  function scopeOf(row: TestRow): CargoTestScope | null {
    const [tag, rest = ''] = [row.id.slice(0, 1), row.id.slice(2)];
    const [pkg, tid, path] = rest.split('|');
    const target = targetOf(pkg, tid);
    switch (tag) {
      case 'p': return { kind: 'package', package: rest };
      case 't': return target ? { kind: 'target', package: pkg, target } : null;
      case 'm': return target ? { kind: 'module', package: pkg, target, module: path } : null;
      case 'c': {
        if (!target) return null;
        const decl = discovered.find(
          (d) => d.package === pkg && targetId(d.target) === tid && d.path === path,
        );
        return {
          kind: 'cases',
          cases: [{ package: pkg, target, path, exact: decl?.kind !== 'parameterized' }],
        };
      }
      default: return null;
    }
  }

  /** The target a row's id names, from whichever side of the tree knows it. */
  function targetOf(pkg: string, tid: string): CargoTestTarget | null {
    const d = discovered.find((x) => x.package === pkg && targetId(x.target) === tid);
    if (d) return d.target;
    const b = [...blocks.values()].find((x) => x.package === pkg && targetId(x.target) === tid);
    return b?.target ?? null;
  }

  return {
    get discovered() { return discovered; },
    get discovering() { return discovering; },
    get running() { return running; },
    get label() { return label; },
    get command() { return command; },
    get widened() { return widened; },
    /** The crate cargo is compiling right now, when it is. */
    get compiling() { return compiling; },
    get exitCode() { return exitCode; },
    get cancelled() { return cancelled; },
    get elapsedMs() { return elapsedMs; },
    get lines() { return lines; },
    get rows() { return rows; },
    get flatRows() { return flatRows; },
    get counts() { return counts(); },
    get hasResults() { return results.size > 0 || blocks.size > 0; },
    get hasFailures() { return counts().failed > 0; },
    get onlyFailed() { return onlyFailed; },
    get sortByTime() { return sortByTime; },
    get selectedId() { return selectedId; },
    get selected(): TestRow | null {
      return flatRows.find((r) => r.id === selectedId) ?? null;
    },
    /** Whether `#[ignore]`d tests are included in a run. */
    get includeIgnored() { return includeIgnored; },

    /** Every group id that is open — the inverse of `collapsed`, which is what the tree widget
     *  wants. Derived from the rows so an id belonging to a run that has been cleared cannot
     *  linger in it. */
    get expandedIds() {
      const out = new Set<string>();
      const walk = (list: TestRow[]) => {
        for (const r of list) {
          if (!r.children.length) continue;
          if (!collapsed.has(r.id)) out.add(r.id);
          walk(r.children);
        }
      };
      walk(rows);
      return out;
    },

    isCollapsed(id: string) { return collapsed.has(id); },
    toggleCollapsed(id: string) {
      if (collapsed.has(id)) collapsed.delete(id);
      else collapsed.set(id, true);
    },
    expandAll() { collapsed.clear(); },
    /** Collapse to the crates — the level you want to see when a workspace has two hundred rows. */
    collapseAll() {
      collapsed.clear();
      const walk = (list: TestRow[], depth: number) => {
        for (const r of list) {
          if (r.children.length && depth >= 0) collapsed.set(r.id, true);
          walk(r.children, depth + 1);
        }
      };
      walk(rows, 0);
    },

    select(id: string | null) { selectedId = id; },
    setOnlyFailed(v: boolean) { onlyFailed = v; },
    setSortByTime(v: boolean) { sortByTime = v; },
    setIncludeIgnored(v: boolean) { includeIgnored = v; },

    attach,
    discover,
    run,
    stop,

    /** Run whatever a row stands for — see {@link TestTreeStore.runRow} for why the panel asks
     *  this way rather than naming a scope itself. */
    runRow(root: string, row: TestRow) {
      const scope = scopeOf(row);
      return scope ? run(root, scope) : Promise.resolve();
    },
    runAll(root: string) { return run(root, { kind: 'workspace' }); },
    runPackage(root: string, pkg: string) { return run(root, { kind: 'package', package: pkg }); },
    runCases(root: string, cases: RustCaseRef[]) {
      return cases.length ? run(root, { kind: 'cases', cases }) : Promise.resolve();
    },

    rerun() {
      if (!lastScope || !lastRoot) return Promise.resolve();
      return run(lastRoot, lastScope);
    },
    rerunFailed() {
      const cases = failedCases();
      if (!cases.length || !lastRoot) return Promise.resolve();
      return run(lastRoot, { kind: 'cases', cases });
    },

    /** The tests declared in `file` — what "run the test at the caret" resolves in. */
    testsInFile(file: string): DiscoveredRustTest[] {
      const norm = file.replace(/\\/g, '/');
      return discovered.filter((d) => d.file === norm);
    },
    /** The tests under a directory — the project tree's "Run tests in …". */
    testsUnder(dir: string): DiscoveredRustTest[] {
      const norm = dir.replace(/\\/g, '/').replace(/\/$/, '');
      return discovered.filter((d) => d.file.startsWith(`${norm}/`));
    },
    /** The case ref that runs one discovered test. */
    caseRefOf(d: DiscoveredRustTest): RustCaseRef {
      return {
        package: d.package,
        target: d.target,
        path: d.path,
        exact: d.kind !== 'parameterized',
      };
    },

    clear() {
      flushPending();
      pendingCases.clear();
      pendingMessages.clear();
      results.clear();
      messages.clear();
      blocks.clear();
      lines = [];
      exitCode = null;
      cancelled = false;
      totals = null;
      widened = null;
      selectedId = null;
      elapsedMs = 0;
      compiling = null;
    },
    reset() {
      flushPending();
      pendingCases.clear();
      pendingMessages.clear();
      discovered = [];
      discoveredRoot = '';
      results.clear();
      messages.clear();
      blocks.clear();
      collapsed.clear();
      lines = [];
      running = false;
      runId = null;
      lastScope = null;
      lastRoot = '';
      exitCode = null;
      cancelled = false;
      totals = null;
      widened = null;
      selectedId = null;
      elapsedMs = 0;
      compiling = null;
      stopTicker();
    },
  };
}

export const bennuCargoTestStore = createCargoTestStore();
