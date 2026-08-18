/**
 * The shape of a test tree — shared by the two runners, because there is one panel.
 *
 * Bennu runs tests for two build systems whose models genuinely differ. Maven's unit of report is
 * a **class** and it writes a file per class; cargo's is a **case** and it writes a line per case.
 * Maven's tree is two levels deep (class → case) and cargo's is four (crate → target → module →
 * test), because a workspace of twenty crates flattened into one list of `tests::works` rows is
 * exactly the thing this file exists to prevent.
 *
 * What must NOT differ is the panel. One tree, one keyboard map, one meaning of red, one Rerun
 * button — so both stores produce {@link TestRow}s and satisfy {@link TestTreeStore}, and
 * `BennuTestView` never learns which one it is drawing.
 *
 * ## Why `depth` rather than a nested `kind` check
 *
 * The old view indented by testing `kind === 'case'`. That works for two levels and breaks at
 * three, and the break is silent: a module row would draw at a crate's indent and the tree would
 * read as flat. `depth` is the row's own answer, so a runner with five levels needs no change here.
 */

import type { RunLogLine } from './run.svelte';

/** What a row is showing. `pending` = declared but not run (yet). */
export type RowStatus = 'passed' | 'failed' | 'error' | 'skipped' | 'running' | 'pending';

/**
 * What kind of thing a row stands for.
 *
 * `class` / `case` are Maven's; `crate` / `target` / `module` are cargo's grouping levels. The
 * view branches on this only for typography — a code identifier gets the code font, a container
 * does not — so a new level costs one entry here and nothing else.
 */
export type RowKind = 'class' | 'case' | 'crate' | 'target' | 'module';

/** One row of the test tree. */
export interface TestRow {
  /** Stable key for `{#each}` and for selection. */
  id: string;
  kind: RowKind;
  /** Indentation level, 0 at the root. See the module doc. */
  depth: number;
  /** What the row reads as. */
  label: string;
  /** The fully-qualified thing the row is about — a class name, or `crate · util::tests`. Shown in
   *  the detail pane's header, where the label alone would be ambiguous. */
  classname: string;
  /**
   * The name a Maven run selects this row's class by (`OrderTest`, `OuterTest$Inner`).
   *
   * Maven-only, and carried on the row rather than derived at the call site: the two spellings of a
   * nested class differ, and deriving one from the other is exactly the step that silently runs the
   * wrong thing. The cargo runner needs no equivalent — its rows are addressed by their id.
   */
  selector?: string;
  /** For a child row, its parent's id — what ← navigates to. */
  parentId?: string;
  /** For a Maven case row, the method name as the report wrote it. */
  method?: string;
  status: RowStatus;
  /** Duration in ms; `null` for a row that has not run. */
  timeMs: number | null;
  flaky: boolean;
  /** Source location, when discovery knows it — this is what makes a row double-clickable. */
  file?: string;
  line?: number;
  offset?: number;
  /** `@Disabled` / `#[ignore]`. */
  disabled: boolean;
  disabledReason?: string | null;
  message?: string | null;
  /** The exception type of a failure (`java.lang.AssertionError`); a panic has none. */
  errorKind?: string | null;
  trace?: string | null;
  /** Whatever the group printed, hung off its row. */
  systemOut?: string | null;
  /** A short badge — `async`, `bench`, `doc`, `2×`. Purely informational. */
  tag?: string | null;
  children: TestRow[];
  /** For a group row: how the rows under it came out. */
  counts?: { total: number; bad: number; skipped: number };
}

/** Whether a status should read as a failure. */
export function isBad(status: RowStatus): boolean {
  return status === 'failed' || status === 'error';
}

/** `1.2s` / `340ms` / `1m 05s` — the same reading the Build panel uses. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m ${String(s).padStart(2, '0')}s`;
}

/**
 * The surface the test panel drives — everything `BennuTestView`, `BennuTestActions` and
 * `BennuTestSummary` touch, and nothing else.
 *
 * Both stores implement it, and `activeTestStore` picks between them by project kind. That is why
 * `runRow` exists instead of the panel calling `runClass` / `runCase`: **what a row means to a
 * runner is the runner's business**. A Maven class row runs by its Surefire selector, a cargo
 * module row runs by a package plus a target plus a filter, and a view that knew either of those
 * would have to know both.
 */
export interface TestTreeStore {
  readonly discovering: boolean;
  readonly running: boolean;
  /** What the current (or last) run was about, in words. */
  readonly label: string;
  /** Set when a selection was too large to express and the run was widened — the panel must show
   *  it, because the user asked for a subset and got a superset. */
  readonly widened: string | null;
  /** The run's transcript, as the console renders it. */
  readonly lines: RunLogLine[];
  readonly rows: TestRow[];
  readonly flatRows: TestRow[];
  /**
   * The tally.
   *
   * `errored` is distinct from `failed` on the Maven side — a class that threw before asserting is
   * a different fact from one that disagreed — and is always 0 for cargo, which has no such
   * distinction: a panic is a panic. Kept in the shape rather than dropped so the Java panel does
   * not lose a number it earned.
   */
  readonly counts: {
    total: number;
    passed: number;
    failed: number;
    errored: number;
    skipped: number;
  };
  readonly hasResults: boolean;
  readonly hasFailures: boolean;
  readonly onlyFailed: boolean;
  readonly sortByTime: boolean;
  readonly selectedId: string | null;
  readonly selected: TestRow | null;
  readonly elapsedMs: number;
  readonly exitCode: number | null;
  readonly cancelled: boolean;

  /**
   * The ids that are expanded — what the shared `Tree` widget takes.
   *
   * Both stores remember the *collapsed* set instead, because a row that has never been touched
   * should be open and a collapsed-set is the only spelling where "unknown" means "open". This
   * getter inverts it against the rows that actually exist.
   */
  readonly expandedIds: Set<string>;

  isCollapsed(id: string): boolean;
  toggleCollapsed(id: string): void;
  expandAll(): void;
  collapseAll(): void;
  select(id: string | null): void;
  setOnlyFailed(v: boolean): void;
  setSortByTime(v: boolean): void;

  discover(root: string, force?: boolean): Promise<void>;
  /** Run whatever this row stands for. */
  runRow(root: string, row: TestRow): Promise<void>;
  runAll(root: string): Promise<void>;
  rerun(): Promise<void>;
  rerunFailed(): Promise<void>;
  stop(): Promise<void>;
  clear(): void;
  reset(): void;
}
