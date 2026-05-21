import { invoke } from '@tauri-apps/api/core';
import type { RepoStatus, CherryPickResult } from '../types/git';
import { invalidateTabCache } from './cache-invalidate';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const getStatus = (tabId: string) =>
  invoke<RepoStatus>('get_status', { tabId });

export const getGitCommitTemplate = (tabId: string) =>
  invoke<string | null>('get_git_commit_template', { tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const stageFile = async (tabId: string, path: string): Promise<void> => {
  await invoke<void>('stage_file', { tabId, path });
  invalidateTabCache(tabId);
};

export const unstageFile = async (tabId: string, path: string): Promise<void> => {
  await invoke<void>('unstage_file', { tabId, path });
  invalidateTabCache(tabId);
};

export const stageAll = async (tabId: string): Promise<void> => {
  await invoke<void>('stage_all', { tabId });
  invalidateTabCache(tabId);
};

export const unstageAll = async (tabId: string): Promise<void> => {
  await invoke<void>('unstage_all', { tabId });
  invalidateTabCache(tabId);
};

export const discardFile = async (tabId: string, path: string): Promise<void> => {
  await invoke<void>('discard_file', { tabId, path });
  invalidateTabCache(tabId);
};

export const discardAll = async (tabId: string): Promise<void> => {
  await invoke<void>('discard_all', { tabId });
  invalidateTabCache(tabId);
};

export const stagePatch = async (tabId: string, patch: string): Promise<void> => {
  await invoke<void>('stage_patch', { tabId, patch });
  invalidateTabCache(tabId);
};

export const commitChanges = async (tabId: string, message: string, amend = false): Promise<string> => {
  const oid = await invoke<string>('commit', { tabId, message, amend });
  invalidateTabCache(tabId);
  return oid;
};

export const cherryPick = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await invoke<CherryPickResult>('cherry_pick', { tabId, oid });
  invalidateTabCache(tabId);
  return result;
};

export const revertCommit = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await invoke<CherryPickResult>('revert_commit', { tabId, oid });
  invalidateTabCache(tabId);
  return result;
};
