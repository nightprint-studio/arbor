/**
 * Bennu unit-test types — the wire shapes of the `tests` domain, in **snake_case** because
 * that is what crosses the seam. Authoritative source: `crates/products/bennu/test` (the
 * pure half) and `crates/products/bennu/be/src/tests.rs` (the runner).
 *
 * Two shapes, and they are deliberately NOT the same one:
 *
 * - {@link DiscoveredTest} is what the sources **declare** — read by parsing, available
 *   before anything runs, and the thing the tree is built from when idle.
 * - {@link TestClassResult} is what a run **produced** — read from Surefire's XML.
 *
 * They do not correspond one-to-one, and code that assumes they do will be wrong on the
 * cases that matter: one `@ParameterizedTest` declaration yields many result cases, a test
 * inherited from an abstract base is declared in one class and reported under another, and a
 * class that failed to initialise reports a case that was never declared at all.
 */

/** Which framework a test class is written against. */
export type TestFramework = 'junit5' | 'junit4' | 'junit3' | 'testng';

/** A declared test method. */
export interface TestMethodInfo {
  name: string;
  /** 1-based line of the method name — the gutter row and the go-to target. */
  line: number;
  /** Byte offset of the method name, for placing the caret exactly. */
  offset: number;
  /** `@Disabled` / `@Ignore` / TestNG's `enabled = false`. */
  disabled: boolean;
  disabled_reason: string | null;
  /** One declaration, many executions (`@ParameterizedTest`, `@RepeatedTest`). */
  dynamic: boolean;
}

/** A declared test class, plus the Maven module it belongs to. */
export interface DiscoveredTest {
  /** Dotted fully-qualified name (`com.acme.Outer.Nested`). */
  fqcn: string;
  /** The name Surefire selects by (`OrderTest`, `OuterTest$Inner`). */
  selector: string;
  package: string;
  /** Absolute path, forward-slashed. */
  file: string;
  line: number;
  offset: number;
  framework: TestFramework;
  /** A shared base class: real, but Surefire cannot instantiate it. */
  is_abstract: boolean;
  disabled: boolean;
  methods: TestMethodInfo[];
  /** Maven module relative to the project root; `null` for the root module. */
  module: string | null;
}

/** How one case ended. `failed` is a wrong answer, `error` is a broken run. */
export type TestStatus = 'passed' | 'failed' | 'error' | 'skipped';

/** One executed case. */
export interface TestCaseResult {
  name: string;
  classname: string;
  status: TestStatus;
  time_ms: number;
  message: string | null;
  /** The exception type (`java.lang.AssertionError`). */
  kind: string | null;
  trace: string | null;
  /** It failed and then passed on a rerun. */
  flaky: boolean;
}

/** One class's report — the contents of one `TEST-*.xml`. */
export interface TestClassResult {
  classname: string;
  total: number;
  failures: number;
  errors: number;
  skipped: number;
  time_ms: number;
  cases: TestCaseResult[];
  system_out: string | null;
  system_err: string | null;
}

/** One case, by class selector name + method. */
export interface TestCaseRef {
  class: string;
  method: string;
}

/** What to run. Mirrors the Rust enum's `{ kind, … }` tagging. */
export type TestScope =
  | { kind: 'all' }
  | { kind: 'module'; module: string }
  | { kind: 'classes'; classes: string[] }
  | { kind: 'cases'; cases: TestCaseRef[] };

/** The handle correlating a live run with its event stream. */
export interface TestRunHandle {
  run_id: string;
  /** What is running, in words. */
  label: string;
  /** Set when the selection was too big for one command line and the run was widened. */
  widened: string | null;
}

/** Maven's own tally, read off the console — a cross-check against the reports. */
export interface TestRunTotals {
  run: number;
  failures: number;
  errors: number;
  skipped: number;
}
