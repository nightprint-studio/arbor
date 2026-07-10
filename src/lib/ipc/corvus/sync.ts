import { corvus } from '../rpc';
import type {
  SyncConfig,
  SyncStatus,
  PullPlan,
  PullSelections,
  PullSummary,
} from '$lib/types/corvus/sync';

// Thin wrappers over the corvus-be `sync` RPC surface. Param keys are the
// backend handler arg names (snake_case), forwarded verbatim.

export function getSyncConfig(): Promise<SyncConfig> {
  return corvus<SyncConfig>('get_sync_config');
}

export function setSyncConfig(config: SyncConfig): Promise<void> {
  return corvus<void>('set_sync_config', { config });
}

export function syncStatus(): Promise<SyncStatus> {
  return corvus<SyncStatus>('sync_status');
}

/** Enable sync: resolve/create the private repo and do a first push. */
export function syncEnable(provider: string, repoName: string | null): Promise<SyncStatus> {
  return corvus<SyncStatus>('sync_enable', { provider, repo_name: repoName });
}

export function syncDisable(): Promise<SyncStatus> {
  return corvus<SyncStatus>('sync_disable');
}

export function syncPushNow(): Promise<SyncStatus> {
  return corvus<SyncStatus>('sync_push_now');
}

export function syncPullPreview(): Promise<PullPlan> {
  return corvus<PullPlan>('sync_pull_preview');
}

export function syncPullApply(selections: PullSelections): Promise<PullSummary> {
  return corvus<PullSummary>('sync_pull_apply', { selections });
}
