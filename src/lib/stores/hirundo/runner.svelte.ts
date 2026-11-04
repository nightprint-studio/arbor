/**
 * Hirundo runner — send every request of a collection or folder in tree order, run each one's tests, keep
 * the results, and remember the run (the project's history, in the profile).
 *
 * Values a script sets (`hirundo.env.set`) become local variables, so the next request in the run sees
 * them: that is how a login feeds the requests after it. One run at a time; `stop()` finishes the request
 * in flight and does not start the next. A run started by a schedule never confirms a protected
 * environment: the requests that would have asked are reported as not sent.
 */
import { requestGet, requestSend, runRecord, runsClear, runsList, testsRun } from '$lib/ipc/hirundo';
import { findContainer, requestsUnder } from '$lib/components/hirundo/request-model';
import { hirundoStore } from './workspace.svelte';
import type { Check, Method, RunRecord } from '$lib/types/hirundo';

export interface RunRow {
  path: string;
  name: string;
  method: Method;
  status: number | null;
  time_ms: number | null;
  checks: Check[];
  /** Why nothing came back (not sent, failed to connect…). */
  error: string | null;
  passed: boolean;
}

export interface RunOptions {
  /** Ask nothing about a protected environment: the user said yes before the run began. */
  confirmed: boolean;
  /** The environment to run in; the active one when empty. */
  envId?: string;
  /** `manual`, or the name of the schedule. */
  source?: string;
}

function createHirundoRunner() {
  let running = $state(false);
  let label = $state('');
  let rows = $state<RunRow[]>([]);
  let total = $state(0);
  let startedAt = $state(0);
  let finishedAt = $state(0);
  let history = $state<RunRecord[]>([]);
  let stopRequested = false;
  let counter = 0;

  const passed = $derived(rows.filter((r) => r.passed).length);
  const failed = $derived(rows.length - passed);

  async function loadHistory() {
    const project = hirundoStore.project;
    history = project ? await runsList(project).catch(() => []) : [];
  }

  async function clearHistory() {
    const project = hirundoStore.project;
    if (!project) return;
    await runsClear(project);
    history = [];
  }

  /** Run `containerPath`; resolves with the counts, or `null` when it could not start. */
  async function run(containerPath: string, opts: RunOptions): Promise<{ passed: number; failed: number } | null> {
    const project = hirundoStore.project;
    if (!project || running) return null;
    const container = findContainer(hirundoStore.tree, containerPath);
    if (!container) return null;
    const requests = requestsUnder(container.children);
    running = true;
    stopRequested = false;
    label = container.name;
    rows = [];
    total = requests.length;
    startedAt = Date.now();
    finishedAt = 0;
    const envName = (opts.envId ? hirundoStore.envs.environments.find((e) => e.id === opts.envId) : hirundoStore.activeEnv)?.name ?? 'no environment';

    for (const node of requests) {
      if (stopRequested) break;
      rows.push(await runOne(project, node.path, node.name, node.method, opts));
    }
    finishedAt = Date.now();
    running = false;

    const record: RunRecord = {
      at_ms: startedAt, label, env_name: envName, source: opts.source ?? 'manual', duration_ms: finishedAt - startedAt,
      rows: rows.map((r) => ({
        path: r.path, name: r.name, method: r.method, status: r.status, time_ms: r.time_ms, passed: r.passed,
        problems: [...(r.error ? [r.error] : []), ...r.checks.filter((c) => !c.passed).map((c) => (c.detail ? `${c.line} — ${c.detail}` : c.line))],
      })),
    };
    void runRecord(project, record).then(() => (history = [record, ...history].slice(0, 50))).catch(() => {});
    return { passed, failed };
  }

  async function runOne(project: string, path: string, name: string, method: Method, opts: RunOptions): Promise<RunRow> {
    const row: RunRow = { path, name, method, status: null, time_ms: null, checks: [], error: null, passed: false };
    try {
      const request = await requestGet(project, path);
      const result = await requestSend({ project, request, path, env_id: opts.envId || null }, `run-${++counter}`, opts.confirmed);
      if (result.outcome === 'unresolved') {
        row.error = `Not sent — no value for ${[...result.names, ...result.missing_secrets].join(', ')}`;
      } else if (result.outcome === 'needs_confirmation') {
        row.error = 'Not sent — the environment asks for confirmation';
      } else {
        row.status = result.response.status;
        row.time_ms = result.response.time_ms;
        if (request.tests.trim()) {
          const report = await testsRun(request.tests, result.response);
          row.checks = report.checks;
          if (report.set_vars.length) {
            const local = hirundoStore.envs.local;
            const vars = local.vars.filter((v) => !report.set_vars.some(([k]) => k === v.key));
            for (const [key, value] of report.set_vars) vars.push({ key, value, description: 'set by a test', enabled: true });
            await hirundoStore.setLocal({ ...local, vars });
          }
        }
        // No tests: a request "passes" when the server did not answer with an error.
        row.passed = row.checks.length ? row.checks.every((c) => c.passed) : result.response.status < 400;
      }
    } catch (e) {
      row.error = String(e);
    }
    return row;
  }

  return {
    get running() { return running; },
    get label() { return label; },
    get rows() { return rows; },
    get total() { return total; },
    get passed() { return passed; },
    get failed() { return failed; },
    get history() { return history; },
    get elapsedMs() { return (finishedAt || Date.now()) - startedAt; },
    run, loadHistory, clearHistory,
    stop() { stopRequested = true; },
  };
}

export const hirundoRunner = createHirundoRunner();
