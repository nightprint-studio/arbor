/**
 * The debugger's wire shapes — the mirror of `bennu_proto::contract`'s debug section.
 *
 * Snake_case, because these come off the RPC seam verbatim; the store converts where a
 * camelCase shape is worth having (breakpoints, which the editor edits) and passes the rest
 * through as they are (frames and values, which are read once and rendered).
 */

/** A line breakpoint, as the gutter holds it.
 *
 *  Identified by **file and line** and not by a class and a bytecode index: that is what was
 *  clicked, it survives a rebuild, and it is what gets persisted. Turning it into something
 *  the VM understands is the backend's job, redone on every launch. */
export interface BreakpointDto {
  /** Absolute path, forward slashes. */
  file: string;
  /** 1-based line. */
  line: number;
  /** A disabled breakpoint is remembered but not installed. */
  enabled: boolean;
}

/** A breakpoint on a **throw** rather than on a line.
 *
 *  `caught` and `uncaught` are separate questions: an uncaught throw is a crash and is worth
 *  stopping on always, a caught one is ordinary control flow in any framework that uses
 *  exceptions for flow. */
export interface ExceptionBreakpointDto {
  /** Fully-qualified throwable. Empty = any. */
  class: string;
  caught: boolean;
  uncaught: boolean;
  enabled: boolean;
}

/** The per-repo `[bennu.debug]` section. */
export interface DebugConfigDto {
  breakpoints: BreakpointDto[];
  exceptions: ExceptionBreakpointDto[];
  /** Watch expressions, in the order they were added. Persisted for the same reason a
   *  breakpoint is — a watch is set up once and wanted back on the next launch. */
  watches: string[];
}

/** What a live VM made of a breakpoint — what the gutter draws solid, hollow, or annotated. */
export interface BreakpointStatusDto {
  file: string;
  line: number;
  verified: boolean;
  /** Why it isn't verified, or where it really bound. "The class isn't loaded yet" (which
   *  resolves itself) reads very differently from "that line has no code" (which never will). */
  message: string;
}

/** One frame of a suspended thread's stack. Deliberately the same shape a stack-trace frame in
 *  the console has: clicking either means the same thing, and a library frame resolves through
 *  the same path rather than through a second one that drifts. */
export interface StackFrameDto {
  /** 0 = the innermost frame, where execution is. */
  index: number;
  /** Fully-qualified declaring class, `$` and all. **Empty on a native frame** — see `name`. */
  class: string;
  method: string;
  /**
   * The frame's whole name, when the debugger gives one string rather than a class and a method.
   *
   * The one place the two debuggers differ. A JVM frame has a declaring class and a method; a native
   * one has `geode::mine::dig`, or `core::ops::function::FnOnce::call_once{{vtable.shim}}` for a
   * synthetic frame — and splitting that at the last `::` to invent a class produces nonsense on
   * exactly the frames worth reading. So a native frame carries it whole here and leaves `class`
   * empty, and the panel prefers this when it is set.
   *
   * Absent from a Java session's frames, so treat it as optional.
   */
  name?: string;
  line: number | null;
  /** Absolute path, when this project declares the class. Absent for a library frame. */
  file: string | null;
  /** Whether it is this project's own code. */
  project: boolean;
}

/** One row of the variables tree: a variable, a field, an array element or a watch. */
export interface DebugValueDto {
  name: string;
  /** `argument` · `local` · `this` · `field` · `element` · `watch`. */
  kind: string;
  /** The declared type, simple-named. */
  type_name: string;
  /** Already rendered by the backend — `42`, `"hello"`, `null`, `Order@1f3c`, `int[12]`. */
  value: string;
  /** The object handle, when there is more inside. A string, because a JDWP identifier is 64
   *  bits and JSON numbers are not. */
  object: string | null;
}

/** One value and everything under it, as RON-shaped text — what the inspect modal shows. */
export interface DebugDumpDto {
  /** The rendering. RON-*shaped*: a debugger reports a name, a rendered value and children, not a
   *  type system, so the shape is inferred from the children. Reading it is the promise;
   *  round-tripping it through a RON parser is not. */
  text: string;
  /** How many values were visited. */
  nodes: number;
  /** Whether a cap was hit — depth, node count, one container's width, or the time budget. */
  truncated: boolean;
}

/** Where a session is as a whole. */
export interface DebugStatusDto {
  /** The run id the session belongs to — the same id the Run console tab carries. */
  session_id: string;
  status: 'starting' | 'running' | 'paused' | 'terminated';
  /** The VM's own description. Sent once, when it becomes known. */
  vm: string;
  message: string;
  /**
   * Which debugger is underneath, and therefore which language's rules the panel should describe.
   *
   * A Java watch is a path — `order.customer.name`, `items[2]`. A Rust one is that plus a leading
   * `*`, and behind a prefix it can also reach the adapter's own expression evaluators. Those are
   * different affordances to explain, and nothing else in this payload says which applies.
   *
   * Optional because a session that predates the field is a JVM one.
   */
  engine?: 'jvm' | 'native';
  /**
   * A standing caveat about what this session can show, or empty.
   *
   * One situation, and a common one: an LLDB with no Rust formatters renders a `Vec` as a pointer
   * and a length. Shown rather than left to be discovered, because unconfigured and broken look
   * identical from the variables tree.
   */
  note?: string;
}

/** The program stopped: which thread, why, and where it is. */
export interface DebugPauseDto {
  session_id: string;
  thread: string;
  thread_name: string;
  reason: 'breakpoint' | 'step' | 'exception';
  /** The throwable's type, when `reason` is `exception`. */
  exception: string | null;
  frames: StackFrameDto[];
}

/** The breakpoint-verification broadcast. */
export interface DebugBreakpointsEvent {
  session_id: string;
  root: string;
  breakpoints: BreakpointStatusDto[];
}

/** How far a step goes. */
export type StepDepth = 'into' | 'over' | 'out';
