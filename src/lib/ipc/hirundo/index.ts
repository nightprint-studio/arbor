/**
 * Hirundo IPC — served by `hirundo-be` over the rpc seam.
 *
 * Secret variable values are the exception: they go to the shell's keychain with direct commands, never
 * through the backend (see `storeSecret`). The account text comes from the backend (`secretAccount`), which
 * is the one place that knows what a secret is called.
 */

import { invoke } from '@tauri-apps/api/core';
import { bennu, hirundo } from '../rpc';
import type {
  Applied, Auth, Cookie, SseStart, HttpEntry, CollectionNode, DetectedEndpoint, EnvState, EnvType, Environment, HirundoConfig, HttpRequest, KeyValue, Local, Meta, OpenApiOperation, OpenApiPlan, Preview, ResponseData, RunRecord, Schedule, ScheduleView, SendResult, SoapImport, SoapInspect, Target, TestReport, VarScope,
} from '$lib/types/hirundo';

// ── The tree ──────────────────────────────────────────────────────────────────────────────────────

export const collectionsTree = (project: string) => hirundo<CollectionNode[]>('collections_tree', { project });
export const requestGet = (project: string, path: string) => hirundo<HttpRequest>('request_get', { project, path });

/** With a `path` it replaces that file; with a `parent` (collection or folder) it creates one. Returns the path. */
export const requestSave = (project: string, request: HttpRequest, where: { path: string } | { parent: string }) =>
  hirundo<string>('request_save', { project, request, path: 'path' in where ? where.path : null, parent: 'parent' in where ? where.parent : null });

export const requestDuplicate = (project: string, path: string) => hirundo<string>('request_duplicate', { project, path });
export const nodeDelete = (project: string, path: string) => hirundo<void>('node_delete', { project, path });
export const collectionCreate = (project: string, name: string) => hirundo<string>('collection_create', { project, name });
export const folderCreate = (project: string, parent: string, name: string) => hirundo<string>('folder_create', { project, parent, name });
export const metaGet = (project: string, path: string) => hirundo<Meta>('meta_get', { project, path });
export const metaSave = (project: string, path: string, meta: Meta) => hirundo<void>('meta_save', { project, path, meta });

// ── Environments ──────────────────────────────────────────────────────────────────────────────────

export const envState = (project: string) => hirundo<EnvState>('env_state', { project });
export const envTypesSave = (project: string, types: EnvType[]) => hirundo<void>('env_types_save', { project, types });
export const environmentSave = (project: string, environment: Environment) => hirundo<void>('environment_save', { project, environment });
export const environmentDelete = (project: string, id: string) => hirundo<void>('environment_delete', { project, id });
export const localSave = (project: string, local: Local) => hirundo<void>('local_save', { project, local });

// ── Sending ───────────────────────────────────────────────────────────────────────────────────────

export const requestPreview = (target: Target) => hirundo<Preview>('request_preview', { target });
export const requestSend = (target: Target, requestId: string, confirmed = false) =>
  hirundo<SendResult>('request_send', { args: { target, request_id: requestId, confirmed } });
export const requestCancel = (requestId: string) => hirundo<boolean>('request_cancel', { request_id: requestId });

// ── Conversions ───────────────────────────────────────────────────────────────────────────────────

export const curlImport = (text: string) => hirundo<HttpRequest>('curl_import', { text });
export const curlExport = (target: Target, resolved: boolean) => hirundo<string>('curl_export', { target, resolved });
export const soapImport = (source: string) => hirundo<SoapImport>('soap_import', { source });
export const soapInspect = (body: string) => hirundo<SoapInspect>('soap_inspect', { body });
export const openapiRead = (source: string) => hirundo<OpenApiPlan>('openapi_read', { source });
export const openapiApply = (args: { project: string; parent: string | null; name: string; description: string; auth: Auth; vars: KeyValue[]; operations: OpenApiOperation[] }) =>
  hirundo<Applied>('openapi_apply', args);
export const httpImport = (source: string) => hirundo<{ entries: HttpEntry[] }>('http_import', { source });
export const httpExport = (project: string, paths: string[], file: string | null) => hirundo<string>('http_export', { project, paths, file });
export const httpEnvImport = (project: string, dir: string) => hirundo<number>('http_env_import', { project, dir });
export const cookiesList = (project: string, envId: string) => hirundo<Cookie[]>('cookies_list', { project, env_id: envId });
export const cookieSave = (project: string, envId: string, cookie: Cookie) => hirundo<void>('cookie_save', { project, env_id: envId, cookie });
export const cookieDelete = (project: string, envId: string, c: Pick<Cookie, 'name' | 'domain' | 'path'>) => hirundo<void>('cookie_delete', { project, env_id: envId, ...c });
export const cookiesClear = (project: string, envId: string) => hirundo<void>('cookies_clear', { project, env_id: envId });
export const sseStart = (target: Target, streamId: string, confirmed = false, lastEventId: string | null = null) =>
  hirundo<SseStart>('sse_start', { args: { target, stream_id: streamId, confirmed, last_event_id: lastEventId } });
export const textSave = (file: string, text: string) => hirundo<void>('text_save', { file, text });
export const sseStop = (streamId: string) => hirundo<boolean>('sse_stop', { stream_id: streamId });
export const kvParse = (text: string) => hirundo<KeyValue[]>('kv_parse', { text });
export const kvFormat = (rows: KeyValue[]) => hirundo<string>('kv_format', { rows });

// ── Secrets (keychain, via the shell) ─────────────────────────────────────────────────────────────

export const secretAccount = (project: string, scope: VarScope, id: string, name: string) =>
  hirundo<string>('secret_account', { project, scope, id, name });

/** Store (or, with an empty value, delete) a secret variable's value. It never passes through the backend. */
export const storeSecret = (account: string, secret: string) => invoke<void>('hirundo_store_secret', { account, secret });
export const deleteSecret = (account: string) => invoke<void>('hirundo_delete_secret', { account });
export const hasSecret = (account: string) => invoke<boolean>('hirundo_has_secret', { account });

// ── Tests ─────────────────────────────────────────────────────────────────────────────────────────

export const testsRun = (tests: string, response: Pick<ResponseData, 'status' | 'headers' | 'body' | 'time_ms' | 'size_bytes'>) =>
  hirundo<TestReport>('tests_run', { tests, response });

// ── Window config ─────────────────────────────────────────────────────────────────────────────────

export const getConfig = () => hirundo<HirundoConfig>('get_hirundo_config');
export const projectOpened = (project: string) => hirundo<HirundoConfig>('hirundo_project_opened', { project });
export const projectForget = (project: string) => hirundo<HirundoConfig>('hirundo_project_forget', { project });

export const workspaceCreate = (name: string) => hirundo<HirundoConfig>('hirundo_workspace_create', { name });
export const workspaceUpdate = (id: string, name: string, colorIdx: number) => hirundo<HirundoConfig>('hirundo_workspace_update', { id, name, color_idx: colorIdx });
export const workspaceDelete = (id: string) => hirundo<HirundoConfig>('hirundo_workspace_delete', { id });
export const workspaceSwitch = (id: string) => hirundo<HirundoConfig>('hirundo_workspace_switch', { id });
export const workspaceAddProject = (id: string, project: string) => hirundo<HirundoConfig>('hirundo_workspace_add_project', { id, project });
export const workspaceRemoveProject = (id: string, project: string) => hirundo<HirundoConfig>('hirundo_workspace_remove_project', { id, project });
export const workspaceMove = (id: string, gap: number) => hirundo<HirundoConfig>('hirundo_workspace_move', { id, gap });
export const workspaceMoveProject = (fromId: string, project: string, toId: string, gap: number) =>
  hirundo<HirundoConfig>('hirundo_workspace_move_project', { from_id: fromId, project, to_id: toId, gap });
/** A project: a folder with its `.hirundo/` inside. `name` empty turns `location` itself into one. Returns its path. */
export const projectCreate = (location: string, name: string) => hirundo<string>('hirundo_project_create', { location, name });
export const detectedRootsSet = (roots: string[]) => hirundo<HirundoConfig>('hirundo_detected_roots_set', { roots });

// ── Detected from code (served by Bennu) ──────────────────────────────────────────────────────────

export const endpointSchema = (root: string, id: string) => bennu<EndpointSchema>('endpoint_schema', { root, id });
export const endpointsDetected = (root: string) => bennu<DetectedEndpoint[]>('endpoints_detected', { root });

// ── Schedules and the runner's history ────────────────────────────────────────────────────────────

export const schedulesList = (project: string) => hirundo<ScheduleView[]>('schedules_list', { project });
export const scheduleSave = (project: string, schedule: Schedule) => hirundo<void>('schedule_save', { project, schedule });
export const scheduleDelete = (project: string, id: string) => hirundo<void>('schedule_delete', { project, id });
export const schedulesDue = (project: string) => hirundo<Schedule[]>('schedules_due', { project });
export const scheduleRan = (project: string, id: string, passed: boolean, summary: string) => hirundo<void>('schedule_ran', { project, id, passed, summary });
export const runsList = (project: string) => hirundo<RunRecord[]>('runs_list', { project });
export const runRecord = (project: string, record: RunRecord) => hirundo<void>('run_record', { project, record });
export const runsClear = (project: string) => hirundo<void>('runs_clear', { project });

// ── .http files, run from Bennu ───────────────────────────────────────────────────────────────────

export interface HttpOutlineEntry {
  title: string;
  method: Method;
  url: string;
  start_line: number;
  request_line: number;
  end_line: number;
}

export interface HttpEnvBrief {
  name: string;
  type_id: string;
  protected: boolean;
  vars: string[];
}

export interface HttpDiag {
  line: number;
  start: number;
  end: number;
  error: boolean;
  message: string;
}

export type HttpRunResult =
  | { outcome: 'sent'; name: string; method: Method; url: string; response: ResponseData; tests: string; warnings: string[] }
  | { outcome: 'unresolved'; name: string; names: string[] }
  | { outcome: 'needs_confirmation'; name: string; env: string; method: Method; url: string };

export const httpOutline = (text: string) => hirundo<HttpOutlineEntry[]>('http_outline', { text });
export const httpEnvs = (dir: string | null) => hirundo<HttpEnvBrief[]>('http_envs', { dir });
export const httpCheck = (text: string, dir: string | null, env: string | null) => hirundo<HttpDiag[]>('http_check', { text, dir, env });
export const httpRun = (args: { text: string; line: number; dir: string | null; env: string | null; request_id: string; confirmed?: boolean }) =>
  hirundo<HttpRunResult>('http_run', { args });
