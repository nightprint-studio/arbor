// IPC wrappers for the cloud-storage Tauri commands.
//
// All commands are 1:1 with `src-tauri/src/commands/cloud_commands.rs`.

import { invoke } from '@tauri-apps/api/core';
import type {
  CloudConnection,
  CloudListPage,
  CloudObject,
  CloudTestReport,
} from '$lib/types/cloud';

// ── Secrets (keyring) ─────────────────────────────────────────────────────

export const cloudSecretSet = (secretRef: string, value: string) =>
  invoke<void>('cloud_secret_set', { secretRef, value });

export const cloudSecretExists = (secretRef: string) =>
  invoke<boolean>('cloud_secret_exists', { secretRef });

export const cloudSecretDelete = (secretRef: string) =>
  invoke<void>('cloud_secret_delete', { secretRef });

// ── Connection probe ──────────────────────────────────────────────────────

export const cloudTestConnection = (conn: CloudConnection, bucket?: string) =>
  invoke<CloudTestReport>('cloud_test_connection', { conn, bucket });

// ── Object operations ─────────────────────────────────────────────────────

export const cloudList = (
  conn: CloudConnection,
  bucket: string,
  prefix?: string,
  limit?: number,
) => invoke<CloudListPage>('cloud_list', { conn, bucket, prefix, limit });

export const cloudStat = (conn: CloudConnection, bucket: string, path: string) =>
  invoke<CloudObject>('cloud_stat', { conn, bucket, path });

export const cloudDelete = (
  conn: CloudConnection,
  bucket: string,
  path: string,
  recursive = false,
) => invoke<void>('cloud_delete', { conn, bucket, path, recursive });

export const cloudCopy = (conn: CloudConnection, bucket: string, src: string, dst: string) =>
  invoke<void>('cloud_copy', { conn, bucket, src, dst });

// ── Transfers (return job_id) ─────────────────────────────────────────────

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

export const cloudConcatFiles = (
  inputs: string[],
  output: string,
  deleteInputs = false,
) => invoke<void>('cloud_concat_files', { inputs, output, deleteInputs });

export const cloudIsCancelled = (streamId: string) =>
  invoke<boolean>('cloud_is_cancelled', { streamId });

// ── OAuth (Google installed-app, loopback :7732) ─────────────────────────

export const cloudGcsOAuthStart = (
  secretRef: string,
  clientId: string,
  clientSecret?: string,
) => invoke<string>('cloud_gcs_oauth_start', {
  secretRef, clientId, clientSecret,
});
