/**
 * Bennu profiler IPC — recording a JVM / Node / native program and reading the recording back.
 *
 * Wire shapes mirror `crates/products/bennu/runtime/be/src/profiler/mod.rs` and the `bennu-profile`
 * crate; field names are snake_case because they cross the RPC seam as-is.
 */

import { bennu } from '../rpc';
import type { RunHandle } from '$lib/types/bennu';

export type ProfileLang = 'java' | 'node' | 'rust';

export type ParseHow =
  | { kind: 'jfr_cli'; jfr: string }
  | { kind: 'helper_source'; java: string }
  | { kind: 'helper_compiled'; java: string; javac: string };

/** Can this JDK record, and how is the recording read. `reason` / `remedy` are shown verbatim. */
export interface JfrAvailability {
  available: boolean;
  how_to_parse: ParseHow | null;
  reason: string;
  remedy: string | null;
  jdk: string;
}

export interface ProfilerDetect {
  java: JfrAvailability;
  jdk_home: string | null;
  can_attach: boolean;
  node: string | null;
  samply: string | null;
}

export interface LocalJvm { pid: number; name: string }

export interface StartPlan {
  vm_args: string[];
  env: [string, string][];
  wrapper: string[];
  attach_start: string[][];
  attach_stop: string[][];
}

export interface ProfileMeta {
  id: string;
  lang: ProfileLang;
  label: string;
  created_ms: number;
  duration_ms: number | null;
  samples: number;
  ms_per_unit: number;
  top_method: string;
}

export interface MethodStat {
  class: string;
  method: string;
  arity: number | null;
  kind: 'root' | 'thread' | 'method' | 'native';
  file: string | null;
  decl_line: number | null;
  self_w: number;
  total_w: number;
  hot_lines: [number, number][];
}

export interface AllocStat { name: string; count: number; bytes: number }
export interface LockStat { monitor: string; site: string; count: number; total_ms: number; max_ms: number }
export interface GcStat { collector: string; count: number; total_pause_ms: number; max_pause_ms: number; top_cause: string }
export interface ThreadStat { name: string; weight: number }

export interface ProfileView {
  meta: ProfileMeta;
  methods: MethodStat[];
  alloc_sites: AllocStat[];
  alloc_types: AllocStat[];
  locks: LockStat[];
  gc: GcStat[];
  threads: ThreadStat[];
  notes: string[];
}

/** The nested tree the flame graph draws (`self` is the node's own weight). */
export interface FlameWire {
  name: string;
  value: number;
  self: number;
  kind: 'root' | 'thread' | 'method' | 'native';
  class?: string;
  file?: string;
  line?: number;
  children: FlameWire[];
}

export interface MethodDelta {
  class: string;
  method: string;
  old_self_pct: number;
  new_self_pct: number;
  delta_pp: number;
  old_self_ms: number;
  new_self_ms: number;
}

export interface MethodMetric {
  line: number;
  name: string;
  self_pct: number;
  total_pct: number;
  self_ms: number;
  total_ms: number;
  alloc_bytes: number;
  alloc_count: number;
  heat: number;
}

export interface LineMetric { line: number; pct: number; heat: number }

export interface FileMetrics {
  recording_id: string | null;
  label: string;
  created_ms: number;
  stale: boolean;
  metrics: { methods: MethodMetric[]; lines: LineMetric[] };
}

/** Event payloads. */
export interface ProfileDoneEvent {
  session_id: string;
  ok: boolean;
  error?: string;
  meta?: ProfileMeta;
}
export interface ProfileProgressEvent { session_id: string; events: number }

export const EVT_PROFILE_DONE = 'arbor://bennu/profile-done';
export const EVT_PROFILE_PROGRESS = 'arbor://bennu/profile-progress';

export const profileDetect = (root: string): Promise<ProfilerDetect> =>
  bennu('bennu_profile_detect', { args: { root } });

export const profileListJvms = (root: string): Promise<LocalJvm[]> =>
  bennu('bennu_profile_list_jvms', { args: { root } });

export function profilePrepare(args: {
  root: string;
  lang: ProfileLang;
  label: string;
  pid?: number;
  samplyPath?: string;
  rateHz?: number;
}): Promise<{ session_id: string; plan: StartPlan }> {
  return bennu('bennu_profile_prepare', {
    args: {
      root: args.root,
      lang: args.lang,
      label: args.label,
      pid: args.pid ?? null,
      samply_path: args.samplyPath ?? null,
      rate_hz: args.rateHz ?? null,
    },
  });
}

export const profileRunCommand = (
  root: string,
  sessionId: string,
  argv: string[],
  cwd = '',
): Promise<RunHandle> =>
  bennu('bennu_profile_run_command', { args: { root, session_id: sessionId, argv, cwd } });

export const profileAttachStart = (sessionId: string): Promise<void> =>
  bennu('bennu_profile_attach_start', { args: { session_id: sessionId } });

export const profileAttachStop = (sessionId: string): Promise<void> =>
  bennu('bennu_profile_attach_stop', { args: { session_id: sessionId } });

export const profileDumpRunning = (sessionId: string, mainClass: string): Promise<void> =>
  bennu('bennu_profile_dump_running', { args: { session_id: sessionId, main_class: mainClass } });

export const profileFinish = (sessionId: string): Promise<void> =>
  bennu('bennu_profile_finish', { args: { session_id: sessionId } });

export const profileImport = (root: string, path: string, lang?: ProfileLang): Promise<string> =>
  bennu('bennu_profile_import', { args: { root, path, lang: lang ?? null } });

export const profileList = (root: string): Promise<ProfileMeta[]> =>
  bennu('bennu_profile_list', { args: { root } });

export const profileGet = (root: string, id: string): Promise<ProfileView> =>
  bennu('bennu_profile_get', { args: { root, id } });

export const profileFlame = (root: string, id: string, thread?: string): Promise<FlameWire> =>
  bennu('bennu_profile_flame', { args: { root, id, thread: thread ?? null } });

export const profileCompare = (root: string, oldId: string, newId: string): Promise<MethodDelta[]> =>
  bennu('bennu_profile_compare', { args: { root, old: oldId, new: newId } });

export const profileDelete = (root: string, id: string): Promise<void> =>
  bennu('bennu_profile_delete', { args: { root, id } });

export const profileFileMetrics = (
  root: string,
  file: string,
  source?: string,
  id?: string,
): Promise<FileMetrics> =>
  bennu('bennu_profile_file_metrics', { args: { root, file, source: source ?? null, id: id ?? null } });

export interface Located { file: string; line: number | null }

/** Where a profiled Java method is declared, or `null` for a class outside the project. */
export const profileLocate = (root: string, cls: string, method: string, arity?: number | null): Promise<Located | null> =>
  bennu('bennu_profile_locate', { args: { root, class: cls, method, arity: arity ?? null } });

// ── samply: the configured path, and installing the official release ─────────────────────────────

export type SamplySource = 'user' | 'managed' | 'path';

export interface SamplyStatus {
  /** A release exists for this OS / architecture. */
  supported: boolean;
  user_path: string;
  resolved: { path: string; source: SamplySource; version: string | null } | null;
  /** Why nothing resolved (e.g. the configured path is gone). */
  problem: string | null;
  managed_version: string | null;
  installing: boolean;
}

/** What the consent dialog shows about the latest release. */
export interface SamplyLatest {
  version: string;
  asset_name: string;
  size: number;
  url: string;
  /** `api` = the release's own digest, `file` = the published .sha256, `none` = nothing to verify against. */
  checksum_source: 'api' | 'file' | 'none';
  sha256: string | null;
  installed_version: string | null;
  update_available: boolean;
  blocker: string | null;
}

export type SamplyInstallEvent =
  | { kind: 'phase'; phase: 'lookup' | 'download' | 'verify' | 'extract' }
  | { kind: 'progress'; done: number; total: number | null }
  | { kind: 'done'; version: string }
  | { kind: 'error'; error: string };

export const EVT_SAMPLY_INSTALL = 'arbor://bennu/samply-install';

export const samplyStatus = (): Promise<SamplyStatus> => bennu('bennu_samply_status', {});

/** Runs `samply --version` on `path`; resolves to the detected version. */
export const samplyValidate = (path: string): Promise<string> => bennu('bennu_samply_validate', { path });

export const getProfilerTools = (): Promise<{ samply_path: string }> => bennu('get_bennu_profiler_tools', {});

export const setProfilerTools = (config: { samply_path: string }): Promise<void> =>
  bennu('set_bennu_profiler_tools', { config });

export const samplyLatest = (): Promise<SamplyLatest> => bennu('bennu_samply_latest', {});

/** Starts the install and returns at once; follow it with `EVT_SAMPLY_INSTALL`. */
export const samplyInstall = (assetName: string, allowUnverified: boolean): Promise<void> =>
  bennu('bennu_samply_install', { args: { asset_name: assetName, allow_unverified: allowUnverified } });

export const samplyRemove = (): Promise<void> => bennu('bennu_samply_remove', {});
