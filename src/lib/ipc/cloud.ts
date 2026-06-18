// IPC wrappers for the cloud-storage commands.
//
// Wave 1 migrated commands route through the platform backend
// (`platform('<method>', { snake_case })`).  The 7 host-dependent transfer
// commands deferred to Wave 3 still use `invoke` directly.

import { invoke } from '@tauri-apps/api/core';
import { platform } from '$lib/ipc/rpc';
import type {
  CloudConnection,
  CloudListPage,
  CloudObject,
  CloudTestReport,
} from '$lib/types/cloud';

// ── Secrets (keyring) — Wave 1 ────────────────────────────────────────────

export const cloudSecretSet = (secretRef: string, value: string) =>
  platform<void>('cloud_secret_set', { secret_ref: secretRef, value });

export const cloudSecretExists = (secretRef: string) =>
  platform<boolean>('cloud_secret_exists', { secret_ref: secretRef });

export const cloudSecretDelete = (secretRef: string) =>
  platform<void>('cloud_secret_delete', { secret_ref: secretRef });

// ── Connection probe — Wave 1 ─────────────────────────────────────────────

export const cloudTestConnection = (conn: CloudConnection, bucket?: string) =>
  platform<CloudTestReport>('cloud_test_connection', { conn, bucket });

// ── Object operations — Wave 1 ────────────────────────────────────────────

export const cloudList = (
  conn: CloudConnection,
  bucket: string,
  prefix?: string,
  limit?: number,
) => platform<CloudListPage>('cloud_list', { conn, bucket, prefix, limit });

export const cloudStat = (conn: CloudConnection, bucket: string, path: string) =>
  platform<CloudObject>('cloud_stat', { conn, bucket, path });

export const cloudDelete = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  recursive = false,
) => platform<void>('cloud_delete', { conn, bucket, path, recursive });

export const cloudCopy = (conn: CloudConnection, bucket: string, src: string, dst: string) =>
  platform<void>('cloud_copy', { conn, bucket, src, dst });

export const cloudConcatFiles = (
  inputs: string[],
  output: string,
  deleteInputs = false,
) => platform<void>('cloud_concat_files', { inputs, output, delete_inputs: deleteInputs });

// ── Cancellation — Wave 1 ─────────────────────────────────────────────────

export const cloudCancel = (streamId: string) =>
  platform<void>('cloud_cancel', { stream_id: streamId });

export const cloudIsCancelled = (streamId: string) =>
  platform<boolean>('cloud_is_cancelled', { stream_id: streamId });

// ── Progress reporters — Wave 1 ───────────────────────────────────────────

export const cloudReportProgress = (
  streamId: string,
  step: string,
  status?: string,
  detail?: string,
) => platform<void>('cloud_report_progress', { stream_id: streamId, step, status, detail });

export const cloudReportDone = (
  streamId: string,
  ok: boolean,
  summary?: string,
  error?: string,
) => platform<void>('cloud_report_done', { stream_id: streamId, ok, summary, error });

// ── Transfers (return job_id) — Wave 3 deferred, still on invoke ──────────

export const cloudDownload = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  local: string,
) => invoke<string>('cloud_download', { conn, bucket, path, local });

export const cloudUpload = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  local: string,
  overwrite = false,
) => invoke<string>('cloud_upload', { conn, bucket, path, local, overwrite });

export const cloudSync = (
  conn: CloudConnection,
  bucket: string,
  remotePrefix: string,
  local: string,
  direction: 'up' | 'down',
  del = false,
) => invoke<string>('cloud_sync', {
  conn, bucket, remotePrefix, local, direction, delete: del,
});

export const cloudDownloadMany = (
  conn: CloudConnection,
  bucket: string,
  paths: string[],
  localDir: string,
  streamId: string,
  parallel?: number,
  opLabel?: string,
) => invoke<string>('cloud_download_many', {
  conn, bucket, paths, localDir, parallel, opLabel, streamId,
});

// ── OAuth (Google installed-app, loopback :7732) — Wave 3 deferred ────────

export const cloudGcsOAuthStart = (
  secretRef: string,
  clientId: string,
  clientSecret?: string,
) => invoke<string>('cloud_gcs_oauth_start', {
  secretRef, clientId, clientSecret,
});
