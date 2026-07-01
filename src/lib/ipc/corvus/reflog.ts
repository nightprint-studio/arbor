import type { ReflogEntry } from '$lib/types/corvus/git';
import { corvus } from '../rpc';

export function getReflog(tabId: string, limit?: number): Promise<ReflogEntry[]> {
  return corvus<ReflogEntry[]>('get_reflog', { tab_id: tabId, limit });
}
