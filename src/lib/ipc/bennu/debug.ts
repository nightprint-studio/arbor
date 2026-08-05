/**
 * Debugger IPC — the `bennu_debug_*` handlers plus the breakpoint pair.
 *
 * Everything below the breakpoints needs a **live session**, and a session is a run: its id is
 * the run id `bennu_run` returned, so the console tab and the debugger are the same thing to
 * everything that has to correlate them.
 *
 * Same convention as the rest of the bennu IPC: one `args` object, snake_case fields.
 */

import { bennu } from '../rpc';
import type { DebugConfigDto, DebugValueDto, StepDepth } from '$lib/types/bennu/debug';

/** The project's persisted `[bennu.debug]` — the breakpoints the gutter draws when a file
 *  opens, and the watches the panel starts with. Wire: `bennu_get_debug_config`. */
export function getDebugConfig(root: string): Promise<DebugConfigDto> {
  return bennu('bennu_get_debug_config', { args: { root } });
}

/**
 * Persist the debug section **and** push its live half to any running session, in that order.
 *
 * One call rather than two: a breakpoint you just added is a breakpoint the running program
 * should respect *and* one that is still there tomorrow, and splitting those into separate
 * verbs is how the two answers start disagreeing.
 *
 * Wire: `bennu_set_debug_config` — `SetDebugConfigArgs { root, config }`.
 */
export function setDebugConfig(root: string, config: DebugConfigDto): Promise<void> {
  return bennu('bennu_set_debug_config', { args: { root, config } });
}

/** Let the program run on. Wire: `bennu_debug_resume` — `SessionArgs { session_id }`. */
export function debugResume(sessionId: string): Promise<void> {
  return bennu('bennu_debug_resume', { args: { session_id: sessionId } });
}

/** One step, at line granularity, passing straight through the JDK and other people's
 *  frameworks. Wire: `bennu_debug_step` — `StepArgs { session_id, depth }`. */
export function debugStep(sessionId: string, depth: StepDepth): Promise<void> {
  return bennu('bennu_debug_step', { args: { session_id: sessionId, depth } });
}

/** Detach: the program keeps running, unsuspended, with no debugger attached. Deliberately
 *  not a kill — stopping the program is the Run console's Stop. Wire: `bennu_debug_detach`. */
export function debugDetach(sessionId: string): Promise<void> {
  return bennu('bennu_debug_detach', { args: { session_id: sessionId } });
}

/**
 * The class-name patterns a step passes straight through, as they currently stand — the
 * configured list, or the backend's defaults when there is none.
 *
 * Asked rather than mirrored: the default list is the backend's judgement and it revises it, so
 * a copy on this side would be a second answer that drifts. Wire: `bennu_step_excludes`.
 */
export function getStepExcludes(): Promise<string[]> {
  return bennu('bennu_step_excludes', { args: {} });
}

/** Mute or unmute this session's breakpoints — still set and still listed, but not installed
 *  in the VM, so the program runs at full speed. For reaching the end of a run without
 *  deleting the breakpoints you will want back. Wire: `bennu_debug_mute`. */
export function debugMute(sessionId: string, muted: boolean): Promise<void> {
  return bennu('bennu_debug_mute', { args: { session_id: sessionId, muted } });
}

/** The variables in scope at a frame of the stopped thread. Wire: `bennu_debug_variables` —
 *  `FrameArgs { session_id, frame }`. */
export function debugVariables(sessionId: string, frame: number): Promise<DebugValueDto[]> {
  return bennu('bennu_debug_variables', { args: { session_id: sessionId, frame } });
}

/** What is inside an object: its fields (its own and its superclasses'), or an array's
 *  elements. `object` is a {@link DebugValueDto.object} handle. Wire: `bennu_debug_expand`. */
export function debugExpand(sessionId: string, object: string): Promise<DebugValueDto[]> {
  return bennu('bennu_debug_expand', { args: { session_id: sessionId, object } });
}

/** Evaluate a watch against a frame. An expression is a **path** — `order`,
 *  `order.customer.name`, `items[2]` — and rejects anything else by name rather than
 *  approximating it. Wire: `bennu_debug_watch` — `WatchArgs { session_id, frame, expression }`. */
export function debugWatch(
  sessionId: string,
  frame: number,
  expression: string,
): Promise<DebugValueDto> {
  return bennu('bennu_debug_watch', { args: { session_id: sessionId, frame, expression } });
}
