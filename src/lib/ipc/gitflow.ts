import { invoke } from '@tauri-apps/api/core';
import type { GitFlowConfig, GitFlowStatus, FlowFinishResult, FlowStartResult } from '../types/git';
import { invalidateTabCache } from './cache-invalidate';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const getGitFlowConfig = (tabId: string) =>
  invoke<GitFlowConfig>('get_gitflow_config', { tabId });

export const getGitFlowGlobalConfig = () =>
  invoke<GitFlowConfig>('get_gitflow_global_config');

export const gitFlowGetStatus = (tabId: string) =>
  invoke<GitFlowStatus>('gitflow_get_status', { tabId });

export const hasGitFlowRepoOverride = (tabId: string) =>
  invoke<boolean>('has_gitflow_repo_override', { tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const setGitFlowGlobalConfig = async (config: GitFlowConfig): Promise<void> => {
  await invoke<void>('set_gitflow_global_config', { config });
  // global config change — no specific tab to invalidate
};

export const setGitFlowRepoConfig = async (tabId: string, config: GitFlowConfig): Promise<void> => {
  await invoke<void>('set_gitflow_repo_config', { tabId, config });
  invalidateTabCache(tabId);
};

export const clearGitFlowRepoConfig = async (tabId: string): Promise<void> => {
  await invoke<void>('clear_gitflow_repo_config', { tabId });
  invalidateTabCache(tabId);
};

export const gitFlowInit = async (tabId: string): Promise<void> => {
  await invoke<void>('gitflow_init', { tabId });
  invalidateTabCache(tabId);
};

export const gitFlowInitCreateMain = async (tabId: string, fromInitial: boolean): Promise<void> => {
  await invoke<void>('gitflow_init_create_main', { tabId, fromInitial });
  invalidateTabCache(tabId);
};

export const gitFlowFeatureStart = async (tabId: string, name: string): Promise<FlowStartResult> => {
  const r = await invoke<FlowStartResult>('gitflow_feature_start', { tabId, name });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowFeatureFinish = async (tabId: string, name: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await invoke<FlowFinishResult>('gitflow_feature_finish', { tabId, name, forcePr });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowReleaseStart = async (tabId: string, version: string): Promise<FlowStartResult> => {
  const r = await invoke<FlowStartResult>('gitflow_release_start', { tabId, version });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowReleaseFinish = async (tabId: string, version: string, tagMessage: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await invoke<FlowFinishResult>('gitflow_release_finish', { tabId, version, tagMessage, forcePr });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowHotfixStart = async (tabId: string, name: string): Promise<FlowStartResult> => {
  const r = await invoke<FlowStartResult>('gitflow_hotfix_start', { tabId, name });
  invalidateTabCache(tabId);
  return r;
};

export const gitFlowHotfixFinish = async (tabId: string, name: string, tagMessage: string, forcePr = false): Promise<FlowFinishResult> => {
  const r = await invoke<FlowFinishResult>('gitflow_hotfix_finish', { tabId, name, tagMessage, forcePr });
  invalidateTabCache(tabId);
  return r;
};
