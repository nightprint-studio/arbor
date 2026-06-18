import type {
  RemoteAccount, RemoteRepo, RemoteTreeEntry, RemoteFileContent
} from '$lib/types/repoBrowser';
import { corvus } from './rpc';

export const rbListAccounts = (): Promise<RemoteAccount[]> =>
  corvus<RemoteAccount[]>('rb_list_accounts');

export const rbListRepos = (provider: string): Promise<RemoteRepo[]> =>
  corvus<RemoteRepo[]>('rb_list_repos', { provider });

export const rbBrowseTree = (
  provider: string, fullName: string, path: string, branch: string
): Promise<RemoteTreeEntry[]> =>
  corvus<RemoteTreeEntry[]>('rb_browse_tree', { provider, full_name: fullName, path, branch });

export const rbGetFileContent = (
  provider: string, fullName: string, path: string, branch: string
): Promise<RemoteFileContent> =>
  corvus<RemoteFileContent>('rb_get_file_content', { provider, full_name: fullName, path, branch });

export const rbDownloadFile = (
  provider: string, fullName: string, path: string, branch: string, destPath: string
): Promise<void> =>
  corvus<void>('rb_download_file', { provider, full_name: fullName, path, branch, dest_path: destPath });
