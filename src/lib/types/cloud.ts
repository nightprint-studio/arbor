// Types for the cloud-storage plugin. Mirrors `src-tauri/src/cloud/types.rs`.
//
// Earmarked for deletion alongside the rest of the host-side cloud module
// when the WASM plugin runtime lands.

export type Provider = 'gcs' | 's3' | 'azblob';

export type GcsAuth =
  | { method: 'sa_file';   path: string }
  | { method: 'sa_inline'; secret_ref: string }
  | { method: 'adc' }
  | { method: 'gcloud_cli' }
  | { method: 'oauth';     secret_ref: string };

export interface S3Auth {
  access_key_id:      string;
  secret_ref:         string;
  region?:            string;
  endpoint?:          string;
  force_path_style?:  boolean;
}

export interface AzBlobAuth {
  account_name: string;
  secret_ref:   string;
  endpoint?:    string;
}

export interface CloudConnection {
  provider:   Provider;
  config_id:  string;
  project_id?: string | null;
  gcs?:       GcsAuth | null;
  s3?:        S3Auth | null;
  azblob?:    AzBlobAuth | null;
}

export interface CloudObject {
  path:           string;
  is_dir:         boolean;
  size?:          number;
  etag?:          string;
  content_type?:  string;
  last_modified?: string;
}

export interface CloudListPage {
  items:     CloudObject[];
  truncated: boolean;
}

export interface CloudTestReport {
  ok:           boolean;
  error?:       string;
  auth_method?: string;
  identity?:    string;
}

export interface CloudProgress {
  job_id:      string;
  config_id:   string;
  kind:        'download' | 'upload' | 'sync';
  bucket:      string;
  path:        string;
  bytes_done:  number;
  bytes_total: number;
  speed_bps:   number;
  eta_sec?:    number;
}

export interface CloudOAuthDone {
  ok:          boolean;
  error?:      string;
  secret_ref?: string;
}

// ── download_many / chunk-merge ───────────────────────────────────────────

export interface CloudManyFileState {
  index:       number;
  path:        string;
  basename:    string;
  local_path:  string;
  bytes_done:  number;
  bytes_total: number;
  status:      'queued' | 'downloading' | 'done' | 'failed' | 'cancelled';
  error?:      string | null;
}

export interface CloudManyAggregate {
  files_done:  number;
  files_total: number;
  bytes_done:  number;
  bytes_total: number;
}

/** Payload of the `cloud-storage:download-many-progress` plugin hook. The
 *  modal listens for it via the contribution registry (via the plugin) or
 *  by mirroring the same hook through a Tauri event bridge. */
export interface CloudDownloadManyProgress {
  stream_id: string;
  op_label:  string;
  /** "download" while sub-downloads are in flight; switches to "merge"
   *  during a chunk-handler merge phase (set by the handler service). */
  phase:     'download' | 'merge';
  files:     CloudManyFileState[];
  aggregate: CloudManyAggregate;
  /** Merge-phase only — handler-supplied free-form text shown under the
   *  phase indicator (e.g. "Concatenating chunk 2/3…"). */
  merge_note?: string | null;
}

export interface CloudDownloadManyDone {
  stream_id:   string;
  ok:          boolean;
  error?:      string | null;
  local_paths: string[];
}
