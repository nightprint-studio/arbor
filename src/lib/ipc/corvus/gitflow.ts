import type { GitFlowConfig, GitFlowStatus, FlowFinishResult, FlowStartResult } from '../../types/corvus/git';
import { invalidateTabCache } from './cache-invalidate';
import { corvus } from '../rpc';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const getGitFlowConfig = (tabId: string) =>
  corvus<GitFlowConfig>('get_gitflow_config', { tab_id: tabId });

export const getGitFlowGlobalConfig = () =>
  corvus<GitFlowConfig>('get_gitflow_global_config');

export const gitFlowGetStatus = (tabId: string) =>
  corvus<GitFlowStatus>('gitflow_get_status', { tab_id: tabId });

export const hasGitFlowRepoOverride = (tabId: string) =>
  corvus<boolean>('has_gitflow_repo_override', { tab_id: tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const setGitFlowGlobalConfig = async (config: GitFlowConfig): Promise<void> => {
  await corvus<void>('set_gitflow_global_config', { config });
  // global config change — no specific tab to invalidate
};

export const setGitFlowRepoConfig = async (tabId: string, config: GitFlowConfig): Promise<void> => {
  await corvus<void>('set_gitflow_repo_config', { tab_id: tabId, config });
  invalidateTabCache(tabId);
};

export const clearGitFlowRepoConfig = async (tabId: string): Promise<void> => {
  await corvus<void>('clear_gitflow_repo_config', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const gitFlowInit = async (tabId: string): Promise<void> => {
  await corvus<void>('gitflow_init', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const gitFlowInitCreateMain = async (tabId: string, fromInitial: boolean): Promise<void> => {
  await corvus<void>('gitflow_init_create_main', { tab_id: tabId, from_initial: fromInitial });
  invalidateTabCache(tabId);
};

export const gitFlowFeatureStart = async (tabId: string, name: string): Promise<FlowStartResult> => {
  const r = await corvus<FlowStartResult>('gitflow_feature_start', { tab_id: tabId, name });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowFeatureFinish = async (tabId: string, name: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await corvus<FlowFinishResult>('gitflow_feature_finish', { tab_id: tabId, name, force_pr: forcePr });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowReleaseStart = async (tabId: string, version: string): Promise<FlowStartResult> => {
  const r = await corvus<FlowStartResult>('gitflow_release_start', { tab_id: tabId, version });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowReleaseFinish = async (tabId: string, version: string, tagMessage: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await corvus<FlowFinishResult>('gitflow_release_finish', { tab_id: tabId, version, tag_message: tagMessage, force_pr: forcePr });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowHotfixStart = async (tabId: string, name: string): Promise<FlowStartResult> => {
  const r = await corvus<FlowStartResult>('gitflow_hotfix_start', { tab_id: tabId, name });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowHotfixFinish = async (tabId: string, name: string, tagMessage: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await corvus<FlowFinishResult>('gitflow_hotfix_finish', { tab_id: tabId, name, tag_message: tagMessage, force_pr: forcePr });
  invalidateTabCache(tabId);
  return r;
};
