// IPC wrappers for the cloud-storage commands.
//
// All commands now route through the platform backend
// (`platform('<method>', { snake_case })`).  Wave 1 covered the
// host-independent commands; Wave 3 migrated the host-dependent transfers.

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

// ── Transfers (return job_id) — Wave 3 ───────────────────────────────────

export const cloudDownload = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  local: string,
) => platform<string>('cloud_download', { conn, bucket, path, local });

export const cloudUpload = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  local: string,
  overwrite = false,
) => platform<string>('cloud_upload', { conn, bucket, path, local, overwrite });

export const cloudSync = (
  conn: CloudConnection,
  bucket: string,
  remotePrefix: string,
  local: string,
  direction: 'up' | 'down',
  del = false,
) => platform<string>('cloud_sync', {
  conn,
  bucket,
  remote_prefix: remotePrefix,
  local,
  direction,
  delete: del,
});

export const cloudDownloadMany = (
  conn: CloudConnection,
  bucket: string,
  paths: string[],
  localDir: string,
  streamId: string,
  parallel?: number,
  opLabel?: string,
  extraSteps?: Array<[string, string]>,
  keepOpen?: boolean,
) => platform<string>('cloud_download_many', {
  conn,
  bucket,
  paths,
  local_dir: localDir,
  stream_id: streamId,
  parallel,
  op_label: opLabel,
  extra_steps: extraSteps,
  keep_open: keepOpen,
});

export const cloudListStream = (
  conn: CloudConnection,
  bucket: string,
  streamId: string,
  prefix?: string,
  cap?: number,
) => platform<string>('cloud_list_stream', {
  conn,
  bucket,
  stream_id: streamId,
  prefix,
  cap,
});

export const cloudSearchStream = (
  conn: CloudConnection,
  bucket: string,
  pattern: string,
  streamId: string,
  rootPrefix?: string,
) => platform<string>('cloud_search_stream', {
  conn,
  bucket,
  pattern,
  stream_id: streamId,
  root_prefix: rootPrefix,
});

// ── OAuth (Google installed-app, loopback :7732) — Wave 3 ────────────────

export const cloudGcsOAuthStart = (
  secretRef: string,
  clientId: string,
  clientSecret?: string,
) => platform<string>('cloud_gcs_oauth_start', {
  secret_ref: secretRef,
  client_id: clientId,
  client_secret: clientSecret,
});
