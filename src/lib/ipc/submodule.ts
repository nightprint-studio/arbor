import type { SubmoduleInfo } from '../types/git';
import { corvus } from './rpc';

// ── Queries ──────────────────────────────────────────────────────────────────

export const listSubmodules = (tabId: string) =>
  corvus<SubmoduleInfo[]>('list_submodules', { tab_id: tabId });

export const submoduleListBranches = (tabId: string, subPath: string) =>
  corvus<string[]>('submodule_list_branches', { tab_id: tabId, sub_path: subPath });

// ── Operations ───────────────────────────────────────────────────────────────

export const submoduleFetch = (tabId: string, subPath: string) =>
  corvus<void>('submodule_fetch', { tab_id: tabId, sub_path: subPath });

export const submodulePull = (tabId: string, subPath: string) =>
  corvus<string>('submodule_pull', { tab_id: tabId, sub_path: subPath });

export const submodulePush = (tabId: string, subPath: string) =>
  corvus<string>('submodule_push', { tab_id: tabId, sub_path: subPath });

export const submoduleCheckout = (tabId: string, subPath: string, branch: string) =>
  corvus<void>('submodule_checkout', { tab_id: tabId, sub_path: subPath, branch });

// ── Legacy (parent-level update) ─────────────────────────────────────────────

export const updateSubmodule = (tabId: string, name: string, recursive = false) =>
  corvus<void>('update_submodule', { tab_id: tabId, name, recursive });

export const updateAllSubmodules = (tabId: string, recursive = false) =>
  corvus<void>('update_all_submodules', { tab_id: tabId, recursive });
