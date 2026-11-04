/** Hirundo — mirrors of `hirundo-model` / `hirundo-core` / `hirundo-be` types. Field names are the wire names. */

export type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS';
export const METHODS: Method[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'];

/** Methods a protected environment asks about. */
export const MUTATING: Method[] = ['POST', 'PUT', 'PATCH', 'DELETE'];

export interface KeyValue {
  key: string;
  value: string;
  description: string;
  enabled: boolean;
}

/** A variable whose value is this marker is kept in the keyring, never in a file. */
export const SECRET_MARK = '@secret';

export type Auth =
  | { type: 'inherit' }
  | { type: 'none' }
  | { type: 'bearer'; token: string }
  | { type: 'basic'; username: string; password: string }
  | { type: 'api_key'; name: string; value: string; location: 'header' | 'query' }
  | { type: 'ws_security'; username: string; password: string };

export type BodyKind = 'none' | 'json' | 'text' | 'xml' | 'form' | 'soap' | 'multipart';

export type SoapVersion = '1.1' | '1.2';

/** Version and action of a call whose body is a SOAP envelope. */
export interface SoapSettings {
  version: SoapVersion;
  action: string;
}

export interface Body {
  kind: BodyKind;
  text: string;
  form: KeyValue[];
}

export interface Settings {
  timeout_ms: number;
  follow_redirects: boolean;
  verify_tls: boolean;
  /** Send the environment's cookies and keep the ones the answer sets. */
  cookie_jar: boolean;
}

export interface HttpRequest {
  name: string;
  order: number;
  method: Method;
  url: string;
  params: KeyValue[];
  headers: KeyValue[];
  auth: Auth;
  body: Body;
  soap: SoapSettings;
  settings: Settings;
  docs: string;
  tests: string;
}

/** What a collection or folder says about everything under it. */
export interface Meta {
  name: string;
  order: number;
  description: string;
  auth: Auth;
  headers: KeyValue[];
  vars: KeyValue[];
  /** The cookie jar is off for every request below. */
  no_cookies: boolean;
}

export type Node =
  | { kind: 'folder'; path: string; name: string; order: number; auth: string; children: Node[] }
  | { kind: 'request'; path: string; name: string; method: Method; order: number; error: string | null };

export interface CollectionNode {
  path: string;
  name: string;
  order: number;
  description: string;
  auth: string;
  children: Node[];
}

export interface EnvType {
  id: string;
  name: string;
  /** `#rgb` or `#rrggbb`. */
  color: string;
  /** Ask before sending a method that changes things. */
  confirm_mutating: boolean;
  /** The variables every environment of this type starts from. */
  vars: KeyValue[];
}

export interface Environment {
  id: string;
  name: string;
  /** May name a type that no longer exists: then it has no colour and no protection. */
  type_id: string;
  vars: KeyValue[];
}

export interface Local {
  /** The environment requests go to; empty for none. */
  active_env: string;
  vars: KeyValue[];
}

export interface EnvState {
  types: EnvType[];
  environments: Environment[];
  local: Local;
}

export type VarScope = 'collection' | 'folder' | 'type' | 'environment' | 'local' | 'runtime';

/** A request and where it stands: project, place in the tree, environment. */
export interface Target {
  project: string;
  request: HttpRequest;
  /** `bandi/auth/login.toml`; absent for a request not saved yet (it inherits nothing). */
  path?: string | null;
  /** Defaults to the active environment. */
  env_id?: string | null;
}

/** The environment a request is about to go to, for the bar that names it. */
export interface EnvBrief {
  id: string;
  name: string;
  type_id: string;
  type_name: string;
  color: string;
  confirm_mutating: boolean;
}

export interface Preview {
  method: Method;
  url: string;
  headers: [string, string][];
  body: string | null;
  /** Variables with no value: the request cannot be sent while there are any. */
  unresolved: string[];
  missing_secrets: string[];
  env: EnvBrief | null;
  needs_confirmation: boolean;
}

export interface ResponseData {
  status: number;
  status_text: string;
  http_version: string;
  headers: [string, string][];
  body: string;
  /** Present when the body is not UTF-8 (an image, a PDF). */
  body_base64: string | null;
  size_bytes: number;
  truncated: boolean;
  time_ms: number;
  final_url: string;
}

export type SendResult =
  | { outcome: 'sent'; response: ResponseData }
  | { outcome: 'unresolved'; names: string[]; missing_secrets: string[] }
  | { outcome: 'needs_confirmation'; env: EnvBrief | null; method: Method; url: string };

// ── SOAP ──────────────────────────────────────────────────────────────────────────────────────────

export interface SoapFault {
  code: string;
  message: string;
  detail: string;
}

export interface SoapInspect {
  fault: SoapFault | null;
  /** The body indented, when it is XML. */
  pretty: string;
}

export interface SoapImport {
  requests: HttpRequest[];
  notes: string[];
}

// ── OpenAPI import ────────────────────────────────────────────────────────────────────────────────

export interface OpenApiOperation {
  /** `GET /pets/{id}`: unique within the plan. */
  id: string;
  tag: string;
  path: string;
  summary: string;
  request: HttpRequest;
}

export interface OpenApiPlan {
  title: string;
  description: string;
  /** The first server of the spec. */
  base_url: string | null;
  /** What the spec's security schemes ask for, for the folder the requests go in. */
  auth: Auth;
  tags: string[];
  operations: OpenApiOperation[];
  warnings: string[];
}

export interface Applied {
  root: string;
  count: number;
}

// ── .http files ───────────────────────────────────────────────────────────────────────────────────

export interface HttpEntry {
  title: string;
  request: HttpRequest;
  body_file: string | null;
  save_to: string | null;
  warnings: string[];
}

// ── Cookies and event streams ─────────────────────────────────────────────────────────────────────

export interface Cookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  expires_ms: number | null;
  secure: boolean;
  http_only: boolean;
  host_only: boolean;
}

/** One thing a stream did, as the backend reports it (`hirundo:sse`). */
export interface SseMessage {
  stream_id: string;
  kind: 'connecting' | 'open' | 'event' | 'error' | 'reconnecting' | 'closed';
  at_ms: number;
  id?: string | null;
  event?: string;
  data?: string;
  retry?: number | null;
  status?: number;
  message?: string;
  reason?: string;
  in_ms?: number;
}

export type SseStart =
  | { outcome: 'started' }
  | { outcome: 'unresolved'; names: string[]; missing_secrets: string[] }
  | { outcome: 'needs_confirmation'; env: EnvBrief | null; method: Method; url: string };

// ── Window config and tests ───────────────────────────────────────────────────────────────────────

/** Per profile, in `hirundo/config.toml`: which project the window reopens on. */
export interface Workspace {
  id: string;
  name: string;
  color_idx: number;
  projects: string[];
}

export interface HirundoConfig {
  last_project: string;
  active_workspace: string;
  recent_projects: string[];
  /** The Bennu projects "Detected from code" lists; empty means just the project open here. */
  detected_roots: string[];
  workspaces: Workspace[];
}

/** One assertion (or `hirundo.expect`) and how it went. */
export interface Check {
  line: string;
  passed: boolean;
  detail: string;
}

export interface TestReport {
  checks: Check[];
  /** What the Lua `hirundo.env.set` was called with, in order. */
  set_vars: [string, string][];
  logs: string[];
}

// ── Detected from code, schedules, runs ───────────────────────────────────────────────────────────

export interface EndpointParam {
  name: string;
  type_text: string;
  binding: string;
  required: boolean;
}

/** A request mapping Bennu found in the code of a project. */
export interface DetectedEndpoint {
  id: string;
  methods: string[];
  path: string;
  controller: string;
  handler: string;
  return_type: string;
  produces: string;
  params: EndpointParam[];
  file: string;
  /** The line of the handler method (1-based). */
  line: number;
}

export type Recurrence =
  | { kind: 'once'; at_ms: number }
  | { kind: 'interval'; secs: number; anchor_ms: number }
  | { kind: 'daily'; hour: number; minute: number }
  | { kind: 'weekly'; days: number[]; hour: number; minute: number }
  | { kind: 'cron'; expr: string };

export interface Schedule {
  id: string;
  name: string;
  target: string;
  env_id: string;
  recurrence: Recurrence;
  missed: 'skip' | 'run_once';
  enabled: boolean;
  created_ms: number;
  last_run_ms: number | null;
  last_result: string;
  last_summary: string;
}

export interface ScheduleView extends Schedule {
  next_run_ms: number | null;
}

export interface RunRowRecord {
  path: string;
  name: string;
  method: string;
  status: number | null;
  time_ms: number | null;
  passed: boolean;
  problems: string[];
}

export interface RunRecord {
  at_ms: number;
  label: string;
  env_name: string;
  source: string;
  duration_ms: number;
  rows: RunRowRecord[];
}
