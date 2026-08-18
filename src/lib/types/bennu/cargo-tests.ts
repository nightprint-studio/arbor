/**
 * Cargo test types — the wire shapes of the `cargo_tests` domain, in **snake_case** because that is
 * what crosses the seam. Authoritative source: `crates/products/bennu/test/src/cargo_*.rs` (the
 * pure half) and `crates/products/bennu/be/src/cargo_tests.rs` (the runner).
 *
 * The same "declared is not executed" split as the Maven side, for the same reason and with one
 * extra twist of its own: an `#[rstest]` is **one** declared function that produces **many** libtest
 * cases named `fn::case_1`, and there is no way to know how many until it runs.
 *
 * A test's identity here is four things — package, target, module path, name — and all four are
 * needed to run it. `cargo test tests::works` in a twenty-crate workspace runs whatever matches
 * everywhere, which is why the tree groups by the first two before it ever shows a name.
 */

/** Which cargo target a test compiles into. Mirrors the Rust enum's `{ kind, name? }` tagging. */
export type CargoTestTarget =
  | { kind: 'lib' }
  | { kind: 'doc' }
  | { kind: 'bin'; name: string }
  | { kind: 'test'; name: string }
  | { kind: 'bench'; name: string }
  | { kind: 'example'; name: string };

/** How a test function is written — which decides how many cases it yields. */
export type RustTestKind = 'test' | 'async' | 'bench' | 'parameterized';

/** A declared `#[test]`, placed in the build. */
export interface DiscoveredRustTest {
  package: string;
  target: CargoTestTarget;
  /** `::`-separated module path holding the function; empty at a crate root. */
  module: string;
  name: string;
  /** `module::name` — how libtest names the case, and what a filter matches. */
  path: string;
  /** Absolute path, forward-slashed. */
  file: string;
  line: number;
  offset: number;
  kind: RustTestKind;
  ignored: boolean;
  /** It passes *by* panicking. */
  should_panic: boolean;
}

/** One case to run. `exact` is false for a parameterized function, whose `path` is a prefix. */
export interface RustCaseRef {
  package: string;
  target: CargoTestTarget;
  path: string;
  exact: boolean;
}

/** What to run. Mirrors the Rust enum's `{ kind, … }` tagging — the five levels of the tree. */
export type CargoTestScope =
  | { kind: 'workspace' }
  | { kind: 'package'; package: string }
  | { kind: 'target'; package: string; target: CargoTestTarget }
  | { kind: 'module'; package: string; target: CargoTestTarget; module: string }
  | { kind: 'cases'; cases: RustCaseRef[] };

/** The handle correlating a live run with its event stream. */
export interface CargoRunHandle {
  run_id: string;
  label: string;
  /** The command line that was actually run — a filter is easy to get subtly wrong and impossible
   *  to diagnose from a result tree alone. */
  command: string;
  widened: string | null;
}

// ── events ───────────────────────────────────────────────────────────────────

/** `arbor://bennu/cargo-test-target` — a test binary started. */
export interface CargoTargetEvent {
  run_id: string;
  /** Which binary of the run this is, counting from 0. Every case and the summary carry the same
   *  index, which is how they are attributed without matching names. */
  index: number;
  /** How many cases libtest says it will run. */
  count: number;
  package: string;
  target: CargoTestTarget | null;
  /** Cargo's own words for the binary, the fallback label when it could not be placed. */
  desc: string;
}

/**
 * `arbor://bennu/cargo-test-case` — one case finished, **or** a failure's captured output.
 *
 * The two arrive separately because libtest prints them separately: the verdict as the test ends,
 * the panic message in a block at the end of the binary's run. An event with a `message` and no
 * `status` **amends** the row a verdict already created.
 */
export interface CargoCaseEvent {
  run_id: string;
  index?: number;
  package?: string;
  target?: CargoTestTarget | null;
  module?: string;
  name?: string;
  /** The libtest path — the key both events agree on. */
  path: string;
  status?: 'passed' | 'failed' | 'error' | 'skipped';
  /** A skip's reason or a bench's timing. */
  note?: string | null;
  /** The captured panic output. */
  message?: string | null;
}

/** `arbor://bennu/cargo-test-target-done` — libtest's own counts for one binary. */
export interface CargoTargetDoneEvent {
  run_id: string;
  index: number;
  result: {
    passed: number;
    failed: number;
    ignored: number;
    measured: number;
    filtered_out: number;
    time_ms: number;
    ok: boolean;
  };
}

/** `arbor://bennu/cargo-test-compiling` — cargo is building. Shown because on a cold workspace the
 *  first seconds of a test run produce no tests at all, and silence there reads as a hang. */
export interface CargoCompilingEvent {
  run_id: string;
  crate: string;
}
