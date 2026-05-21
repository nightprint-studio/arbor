import { invoke } from '@tauri-apps/api/core';
import type { RecoveryConfig, RecoveryEntry, RecoveryRestorePreview } from '$lib/types/git';

/** List all recovery snapshots for the given tab (newest first). */
export function listRecoveryEntries(tabId: string): Promise<RecoveryEntry[]> {
  return invoke<RecoveryEntry[]>('list_recovery_entries', { tabId });
}

/** Preview what restoring a snapshot would change. */
export function previewRecoveryRestore(
  tabId: string,
  entryId: number,
): Promise<RecoveryRestorePreview> {
  return invoke<RecoveryRestorePreview>('preview_recovery_restore', { tabId, entryId });
}

/** Restore a snapshot into the working directory.  A fresh snapshot of the
 * current state is taken first so the restore itself is reversible. */
export function restoreRecoveryEntry(
  tabId: string,
  entryId: number,
): Promise<RecoveryEntry> {
  return invoke<RecoveryEntry>('restore_recovery_entry', { tabId, entryId });
}

/** Drop a snapshot from the journal + delete its pinning ref. */
export function deleteRecoveryEntry(
  tabId: string,
  entryId: number,
): Promise<void> {
  return invoke<void>('delete_recovery_entry', { tabId, entryId });
}

/** Load the persisted snapshot policy (size limit + extension deny-list). */
export function getRecoveryConfig(): Promise<RecoveryConfig> {
  return invoke<RecoveryConfig>('get_recovery_config');
}

/** Persist a new snapshot policy to ~/.config/arbor/config.toml. */
export function setRecoveryConfig(recovery: RecoveryConfig): Promise<void> {
  return invoke<void>('set_recovery_config', { recovery });
}
