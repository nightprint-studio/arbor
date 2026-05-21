import { invoke } from '@tauri-apps/api/core';
import type { PipelineDef, PipelineRun, CiProviderInfo, CiRun, CiJob, CiWorkflow } from '$lib/types/pipeline';
import { invalidateTabCache } from './cache-invalidate';

export function listPipelineDefs(): Promise<PipelineDef[]> {
  return invoke('list_pipeline_defs');
}

export function listPipelineRuns(): Promise<PipelineRun[]> {
  return invoke('list_pipeline_runs');
}

export function getPipelineRun(runId: string): Promise<PipelineRun> {
  return invoke('get_pipeline_run', { runId });
}

export function runPipeline(plugin: string, pipelineId: string, tabId?: string): Promise<string> {
  return invoke('run_pipeline', { plugin, pipelineId, tabId });
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
  return invoke('request_pipeline_run', { plugin, pipelineId, tabId });
}

export function cancelPipelineRun(runId: string): Promise<void> {
  return invoke('cancel_pipeline_run', { runId });
}

/** Resume a failed/paused pipeline run from the step(s) that halted it. */
export function resumePipelineRun(runId: string): Promise<void> {
  return invoke('resume_pipeline_run', { runId });
}

/** Drop a terminal run permanently (removes in-memory entry + on-disk file). */
export function discardPipelineRun(runId: string): Promise<void> {
  return invoke('discard_pipeline_run', { runId });
}

// CI/CD integration
export function getCiProvider(tabId: string): Promise<CiProviderInfo | null> {
  return invoke('get_ci_provider', { tabId });
}

export function fetchCiRuns(tabId: string): Promise<CiRun[]> {
  return invoke('fetch_ci_runs', { tabId });
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
  return invoke('fetch_mr_ci_runs', { tabId, mrNumber, sourceBranch, headSha });
}

export async function retrigerCiRun(tabId: string, runId: string): Promise<void> {
  await invoke('retrigger_ci_run', { tabId, runId });
  invalidateTabCache(tabId);
}

export function fetchCiJobs(tabId: string, runId: string): Promise<CiJob[]> {
  return invoke('fetch_ci_jobs', { tabId, runId });
}

export function listCiWorkflows(tabId: string): Promise<CiWorkflow[]> {
  return invoke('list_ci_workflows', { tabId });
}

/** Returns the new pipeline ID (GitLab) or null (GitHub — no synchronous ID). */
export async function createCiPipeline(
  tabId:      string,
  branch:     string,
  variables:  [string, string][],
  workflowId?: string,
): Promise<string | null> {
  const r = await invoke<string | null>('create_ci_pipeline', { tabId, branch, variables, workflowId });
  invalidateTabCache(tabId);
  return r;
}
