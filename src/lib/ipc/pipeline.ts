import { corvus } from '$lib/ipc/rpc';
import type { PipelineDef, PipelineRun, CiProviderInfo, CiRun, CiJob, CiWorkflow } from '$lib/types/pipeline';
import { invalidateTabCache } from './cache-invalidate';

export function listPipelineDefs(): Promise<PipelineDef[]> {
  return corvus('list_pipeline_defs');
}

export function listPipelineRuns(): Promise<PipelineRun[]> {
  return corvus('list_pipeline_runs');
}

export function getPipelineRun(runId: string): Promise<PipelineRun> {
  return corvus('get_pipeline_run', { run_id: runId });
}

export function runPipeline(plugin: string, pipelineId: string, tabId?: string): Promise<string> {
  return corvus('run_pipeline', { plugin, pipeline_id: pipelineId, tab_id: tabId });
}

/**
 * Ask the def's owning plugin to start a run.
 *
 * If the plugin has registered an `on_pipeline_run_request` handler, the
 * backend delegates to it (the plugin compiles / resolves stages and calls
 * `arbor.pipeline.run` itself) — and we get `null` back because the run id
 * isn't known synchronously here. Otherwise the backend falls through to
 * the legacy `run_pipeline` direct path and returns the new run id.
 *
 * Use this from any UI (panel Play, command palette, …) that triggers a
 * registered pipeline by `(plugin, pipeline_id)`.
 */
export function requestPipelineRun(
  plugin:     string,
  pipelineId: string,
  tabId?:     string,
): Promise<string | null> {
  return corvus('request_pipeline_run', { plugin, pipeline_id: pipelineId, tab_id: tabId });
}

export function cancelPipelineRun(runId: string): Promise<void> {
  return corvus('cancel_pipeline_run', { run_id: runId });
}

/** Resume a failed/paused pipeline run from the step(s) that halted it. */
export function resumePipelineRun(runId: string): Promise<void> {
  return corvus('resume_pipeline_run', { run_id: runId });
}

/** Drop a terminal run permanently (removes in-memory entry + on-disk file). */
export function discardPipelineRun(runId: string): Promise<void> {
  return corvus('discard_pipeline_run', { run_id: runId });
}

// CI/CD integration
export function getCiProvider(tabId: string): Promise<CiProviderInfo | null> {
  return corvus('get_ci_provider', { tab_id: tabId });
}

export function fetchCiRuns(tabId: string): Promise<CiRun[]> {
  return corvus('fetch_ci_runs', { tab_id: tabId });
}

/**
 * Fetch CI runs scoped to a specific PR/MR. Combines two endpoints per
 * provider so it catches runs a plain branch filter would miss:
 *  - GitHub: `/actions/runs?branch=…` ∪ `/actions/runs?head_sha=…`
 *  - GitLab: `/merge_requests/:iid/pipelines` ∪ `/pipelines?ref=…`
 *
 * Pass `headSha` whenever known — covers GitHub fork PRs and `pull_request_target`
 * runs that wouldn't tag the source branch on the run.
 */
export function fetchMrCiRuns(
  tabId:        string,
  mrNumber:     number,
  sourceBranch: string,
  headSha?:     string,
): Promise<CiRun[]> {
  return corvus('fetch_mr_ci_runs', { tab_id: tabId, mr_number: mrNumber, source_branch: sourceBranch, head_sha: headSha });
}

export async function retrigerCiRun(tabId: string, runId: string): Promise<void> {
  await corvus('retrigger_ci_run', { tab_id: tabId, run_id: runId });
  invalidateTabCache(tabId);
}

export function fetchCiJobs(tabId: string, runId: string): Promise<CiJob[]> {
  return corvus('fetch_ci_jobs', { tab_id: tabId, run_id: runId });
}

export function listCiWorkflows(tabId: string): Promise<CiWorkflow[]> {
  return corvus('list_ci_workflows', { tab_id: tabId });
}

/** Returns the new pipeline ID (GitLab) or null (GitHub — no synchronous ID). */
export async function createCiPipeline(
  tabId:      string,
  branch:     string,
  variables:  [string, string][],
  workflowId?: string,
): Promise<string | null> {
  const r = await corvus<string | null>('create_ci_pipeline', { tab_id: tabId, branch, variables, workflow_id: workflowId });
  invalidateTabCache(tabId);
  return r;
}
