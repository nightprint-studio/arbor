import { invoke } from '@tauri-apps/api/core';
import type { ReflogEntry } from '$lib/types/git';

export function getReflog(tabId: string, limit?: number): Promise<ReflogEntry[]> {
  return invoke<ReflogEntry[]>('get_reflog', { tabId, limit });
}
