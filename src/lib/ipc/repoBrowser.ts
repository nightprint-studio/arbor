import { invoke } from '@tauri-apps/api/core';
import type {
  RemoteAccount, RemoteRepo, RemoteTreeEntry, RemoteFileContent
} from '$lib/types/repoBrowser';

export const rbListAccounts = (): Promise<RemoteAccount[]> =>
  invoke<RemoteAccount[]>('rb_list_accounts');

export const rbListRepos = (provider: string): Promise<RemoteRepo[]> =>
  invoke<RemoteRepo[]>('rb_list_repos', { provider });

export const rbBrowseTree = (
  provider: string, fullName: string, path: string, branch: string
): Promise<RemoteTreeEntry[]> =>
  invoke<RemoteTreeEntry[]>('rb_browse_tree', { provider, fullName, path, branch });

export const rbGetFileContent = (
  provider: string, fullName: string, path: string, branch: string
): Promise<RemoteFileContent> =>
  invoke<RemoteFileContent>('rb_get_file_content', { provider, fullName, path, branch });

export const rbDownloadFile = (
  provider: string, fullName: string, path: string, branch: string, destPath: string
): Promise<void> =>
  invoke<void>('rb_download_file', { provider, fullName, path, branch, destPath });
