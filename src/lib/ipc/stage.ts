import { invoke } from '@tauri-apps/api/core';
import { corvus } from './rpc';
import type { RepoStatus, CherryPickResult } from '../types/git';
import { invalidateTabCache } from './cache-invalidate';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const getStatus = (tabId: string) =>
  corvus<RepoStatus>('get_status', { tab_id: tabId });

export const getGitCommitTemplate = (tabId: string) =>
  corvus<string | null>('get_git_commit_template', { tab_id: tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const stageFile = async (tabId: string, path: string): Promise<void> => {
  await corvus<void>('stage_file', { tab_id: tabId, path });
  invalidateTabCache(tabId);
};

export const unstageFile = async (tabId: string, path: string): Promise<void> => {
  await corvus<void>('unstage_file', { tab_id: tabId, path });
  invalidateTabCache(tabId);
};

export const stageAll = async (tabId: string): Promise<void> => {
  await corvus<void>('stage_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const unstageAll = async (tabId: string): Promise<void> => {
  await corvus<void>('unstage_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const discardFile = async (tabId: string, path: string): Promise<void> => {
  await corvus<void>('discard_file', { tab_id: tabId, path });
  invalidateTabCache(tabId);
};

export const discardAll = async (tabId: string): Promise<void> => {
  await corvus<void>('discard_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const stagePatch = async (tabId: string, patch: string): Promise<void> => {
  await corvus<void>('stage_patch', { tab_id: tabId, patch });
  invalidateTabCache(tabId);
};

// `commit` is intentionally still on the legacy `invoke` path: it fires the
// vetoable `on_pre_commit` hook + `on_commit`, which the broker seam does not
// yet support. It stays an inline #[tauri::command] until that pass.
export const commitChanges = async (tabId: string, message: string, amend = false): Promise<string> => {
  const oid = await invoke<string>('commit', { tabId, message, amend });
  invalidateTabCache(tabId);
  return oid;
};

export const cherryPick = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await corvus<CherryPickResult>('cherry_pick', { tab_id: tabId, oid });
  invalidateTabCache(tabId);
  return result;
};

export const revertCommit = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await corvus<CherryPickResult>('revert_commit', { tab_id: tabId, oid });
  invalidateTabCache(tabId);
  return result;
};
