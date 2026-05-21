import { invoke } from '@tauri-apps/api/core';
import type { SubmoduleInfo } from '../types/git';

// ── Queries ──────────────────────────────────────────────────────────────────

export const listSubmodules = (tabId: string) =>
  invoke<SubmoduleInfo[]>('list_submodules', { tabId });

export const submoduleListBranches = (tabId: string, subPath: string) =>
  invoke<string[]>('submodule_list_branches', { tabId, subPath });

// ── Operations ───────────────────────────────────────────────────────────────

export const submoduleFetch = (tabId: string, subPath: string) =>
  invoke<void>('submodule_fetch', { tabId, subPath });

export const submodulePull = (tabId: string, subPath: string) =>
  invoke<string>('submodule_pull', { tabId, subPath });

export const submodulePush = (tabId: string, subPath: string) =>
  invoke<string>('submodule_push', { tabId, subPath });

export const submoduleCheckout = (tabId: string, subPath: string, branch: string) =>
  invoke<void>('submodule_checkout', { tabId, subPath, branch });

// ── Legacy (parent-level update) ─────────────────────────────────────────────

export const updateSubmodule = (tabId: string, name: string, recursive = false) =>
  invoke<void>('update_submodule', { tabId, name, recursive });

export const updateAllSubmodules = (tabId: string, recursive = false) =>
  invoke<void>('update_all_submodules', { tabId, recursive });
