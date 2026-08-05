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
  /** Fully-qualified declaring class, `$` and all. */
  class: string;
  method: string;
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

/** Where a session is as a whole. */
export interface DebugStatusDto {
  /** The run id the session belongs to — the same id the Run console tab carries. */
  session_id: string;
  status: 'starting' | 'running' | 'paused' | 'terminated';
  /** The VM's own description. Sent once, when it becomes known. */
  vm: string;
  message: string;
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
