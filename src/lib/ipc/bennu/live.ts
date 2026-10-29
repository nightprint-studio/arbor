/**
 * Live runs — Bennu starting the project's services, watching them and checking them.
 *
 * Thin `bennu(...)` wrappers over the `live` domain of `bennu-be`, plus the one push topic. The
 * assistant drives the same state through its tools, so what it started shows up here and what is
 * started here can be read back by it.
 *
 * Handlers that can take a while (start a stack, stop, run a scenario) return at once; the result
 * arrives as a refresh event and, for failures, as `notice` in the state.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { bennu } from '../rpc';

export type LivePhase = 'running' | 'ready' | 'exited' | 'stopped';

export interface LiveProcess {
  id: string;
  name: string;
  command: string;
  cwd: string;
  pid: number;
  phase: LivePhase;
  exit_code: number | null;
  ready: boolean;
  started_ms: number;
  ended_ms: number | null;
  ports: number[];
  group: string | null;
  /** The assistant conversation that started it; null when the user did. */
  owner: string | null;
  log_lines: number;
  log_dropped: number;
  note: string | null;
}

export interface LiveReadySpec {
  port?: number | null;
  log?: string | null;
  http?: string | null;
  status?: number | null;
  delay_ms?: number | null;
  timeout_s?: number | null;
}

export interface LiveService {
  name: string;
  command: string;
  cwd: string;
  env: Record<string, string>;
  ports: number[];
  depends_on: string[];
  ready: LiveReadySpec;
  max_minutes: number | null;
}

export interface LiveRecipe {
  name: string;
  description: string;
  services: LiveService[];
  stop_order: string[];
}

export interface LiveRecipes {
  present: boolean;
  path: string;
  recipes: LiveRecipe[];
  errors: string[];
  approved: boolean;
  hash?: string;
}

export interface LiveScenarioLast {
  passed: boolean;
  started_ms: number;
  duration_ms: number;
  failed: number;
}

export interface LiveScenario {
  file: string;
  name: string;
  description: string;
  steps: number;
  recipes_needed: string[];
  errors: string[];
  last: LiveScenarioLast | null;
}

export interface LiveState {
  processes: LiveProcess[];
  recipes: LiveRecipes;
  scenarios: LiveScenario[];
  scenarios_dir: string;
  last_seq: number;
  notice: string | null;
  scenario_running: string | null;
  limits: { max_running: number };
}

export interface LiveLogLine {
  seq: number;
  ts_ms: number;
  stream: 'stdout' | 'stderr' | 'system';
  text: string;
}

export type LiveEventKind = 'log' | 'http_request' | 'http_response' | 'process_started' | 'process_ready' | 'process_exited' | 'marker';

export interface LiveEvent {
  seq: number;
  ts_ms: number;
  kind: LiveEventKind;
  source: string;
  process_id: string | null;
  corr: string | null;
  text: string;
  data?: unknown;
}

export interface LiveAssertion {
  description: string;
  passed: boolean;
  detail: string;
}

export interface LiveStepResult {
  index: number;
  name: string;
  type: string;
  outcome: 'passed' | 'failed' | 'skipped';
  message: string;
  duration_ms: number;
  assertions: LiveAssertion[];
  corr: string | null;
  slice: LiveEvent[];
}

export interface LiveScenarioResult {
  name: string;
  passed: boolean;
  started_ms: number;
  duration_ms: number;
  steps: LiveStepResult[];
}

/** Which groups of the timeline to show. */
export type LiveKindFilter = 'log' | 'http' | 'marker';

export const liveState = (root: string): Promise<LiveState> => bennu('bennu_live_state', { args: { root } });

export const liveLogs = (root: string, id: string, since?: number, tail?: number): Promise<{ process: LiveProcess; lines: LiveLogLine[] }> =>
  bennu('bennu_live_logs', { args: { root, id, since, tail } });

export const liveTimeline = (a: {
  root: string; since_seq?: number; corr?: string; text?: string; source?: string; kinds?: LiveKindFilter[]; limit?: number;
}): Promise<{ events: LiveEvent[]; last_seq: number }> => bennu('bennu_live_timeline', { args: a });

export const liveTrace = (root: string, corr: string, context?: number): Promise<{ corr: string; events: LiveEvent[]; sources: string[] }> =>
  bennu('bennu_live_trace', { args: { root, corr, context } });

export const liveClearTimeline = (root: string): Promise<void> => bennu('bennu_live_clear_timeline', { args: { root } });

/** `approve` = the user confirmed the commands of the recipes file in the panel. */
export const liveRunRecipe = (root: string, name: string, approve: boolean): Promise<void> =>
  bennu('bennu_live_run_recipe', { args: { root, name, approve } });

export const liveStopRecipe = (root: string, name: string): Promise<void> => bennu('bennu_live_stop_recipe', { args: { root, name } });

export const liveStop = (root: string, id: string): Promise<void> => bennu('bennu_live_stop', { args: { root, id } });

export const liveStopAll = (root: string): Promise<void> => bennu('bennu_live_stop_all', { args: { root } });

export const liveRunScenario = (root: string, name: string, approve: boolean): Promise<void> =>
  bennu('bennu_live_run_scenario', { args: { root, name, approve } });

export const liveScenarioResult = (root: string, name: string): Promise<LiveScenarioResult | null> =>
  bennu('bennu_live_scenario_result', { args: { root, name } });

/** Create a starter recipes file or scenario (never overwrites). Returns the path to open. */
export const liveCreateTemplate = (root: string, kind: 'recipes' | 'scenario'): Promise<string> =>
  bennu('bennu_live_create_template', { args: { root, kind } });

const LIVE_TOPIC = 'arbor://bennu/live';

/** The backend says something in the project changed (coalesced, a few times a second at most). */
export const onLiveChanged = (cb: (root: string) => void): Promise<UnlistenFn> =>
  listen<{ root: string }>(LIVE_TOPIC, (e) => cb(e.payload.root));
