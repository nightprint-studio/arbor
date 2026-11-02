/**
 * Bennu Tomcat hot-swap IPC — link a project to a local Tomcat and copy changed JSPs into the
 * deployed (exploded) webapp so Jasper recompiles them on next request. Routes through the generic
 * `bennu(...)` rpc bridge; wire shapes mirror the BE handlers in
 * `crates/products/bennu/runtime/be/src/tomcat.rs` verbatim (snake_case fields under `{ args: … }`).
 *
 * The second half of the file is the **Tomcat run configuration**: what is installed, the download,
 * the project scan, the live `context.xml` preview, the stored passwords and the launch itself —
 * the handlers of `tomcat_run/`.
 */

import { bennu } from '../rpc';
import type { RunConfigDto, RunHandle } from '$lib/types/bennu';

/** The per-repo Tomcat link — persisted in `<repo>/.arbor/config.toml` `[bennu.tomcat]`. */
export interface TomcatConfig {
  /** CATALINA_BASE/HOME dir (holds `webapps/`). Empty ⇒ not linked. */
  tomcat_root: string;
  /** Deployed context dir under `webapps/` (auto-detected when empty). */
  webapp_name: string;
}

/** What {@link detectTomcat} found — drives the settings modal. */
export interface TomcatDetection {
  /** The root looks like a Tomcat (a `webapps/` dir exists). */
  valid: boolean;
  /** The project's webapp source dir, or '' when it isn't a web project. */
  source_webapp: string;
  /** Deployable exploded context names under `webapps/` (system apps excluded). */
  webapps: string[];
  /** Best-match context for this project ('' when ambiguous / none). */
  suggested: string;
  /** JSP-family files under the source webapp dir (a full swap's file count). */
  jsp_count: number;
}

/** The result of a hot-swap. */
export interface HotSwapResult {
  copied: number;
  target_dir: string;
  webapp_name: string;
}

/** Read the per-repo Tomcat link. A never-linked project yields `{ tomcat_root: '', webapp_name: '' }`. */
export function getTomcatConfig(root: string): Promise<TomcatConfig> {
  return bennu('bennu_get_tomcat_config', { args: { root } });
}

/** Persist the per-repo Tomcat link (preserving every other `.arbor/config.toml` section). */
export function setTomcatConfig(root: string, config: TomcatConfig): Promise<void> {
  return bennu('bennu_set_tomcat_config', { args: { root, config } });
}

/** Validate a candidate Tomcat root against the project + resolve the best-match deployed context. */
export function detectTomcat(root: string, tomcatRoot: string): Promise<TomcatDetection> {
  return bennu('bennu_detect_tomcat', { args: { root, tomcat_root: tomcatRoot } });
}

/** Hot-swap the project's JSP(s) into the linked Tomcat. `file` present ⇒ that single JSP; absent ⇒
 *  every JSP under the webapp source dir. Rejects (with a message) when no Tomcat is linked, the
 *  webapp source dir is missing, or the deployed context is ambiguous. The BE also fires a toast. */
export function hotswapJsp(root: string, file?: string): Promise<HotSwapResult> {
  return bennu('bennu_hotswap_jsp', { args: { root, file: file ?? null } });
}

// ── running on Tomcat ─────────────────────────────────────────────────────────────────

/** An unpacked Tomcat: what version it is, and so what it speaks. */
export interface TomcatInstallation {
  version: string;
  /** `CATALINA_HOME`, forward slashes. */
  home: string;
  /** `"9.0"`. */
  line: string;
  /** `"javax"` | `"jakarta"` — the servlet package root. */
  namespace: string;
  min_java: number;
  servlet: string;
}

export interface TomcatLine {
  line: string;
  namespace: string;
  servlet: string;
  min_java: number;
}

/** The folders in force and what is installed — one call, the form shows all of it at once. */
export interface TomcatEnvironment {
  install_dir: string;
  base_dir: string;
  installs: TomcatInstallation[];
  lines: TomcatLine[];
}

export interface TomcatRequirement {
  /** Relative to `java:comp/env/`. */
  name: string;
  /** `"datasource"` | `"env"`. */
  kind: string;
  /** `WEB-INF/web.xml:18 · <resource-ref>`. */
  source: string;
}

/** What the project's own files say about running on Tomcat. */
export interface TomcatProjectScan {
  requirements: TomcatRequirement[];
  /** `"javax"` | `"jakarta"`, or `null` when the sources do not say. */
  namespace: string | null;
  /** The module's `src/main/webapp`, or `null` when it has none. */
  webapp_dir: string | null;
}

export const tomcatEnvironment = (
  root: string,
  installDir: string,
  baseDir: string,
): Promise<TomcatEnvironment> =>
  bennu('bennu_tomcat_environment', { args: { root, install_dir: installDir, base_dir: baseDir } });

/** Read an installation the user pointed at; rejects with what is missing. */
export const tomcatInspect = (home: string): Promise<TomcatInstallation> =>
  bennu('bennu_tomcat_inspect', { args: { home } });

/** Walks the project's tree — call it once when the form opens, not per keystroke. */
export const tomcatScan = (root: string, module: string): Promise<TomcatProjectScan> =>
  bennu('bennu_tomcat_scan', { args: { root, module } });

/** Which of `ports` something is already listening on. */
export const tomcatBusyPorts = (ports: number[]): Promise<number[]> =>
  bennu('bennu_tomcat_ports', { args: { ports } });

/** The stable releases of a line, newest first (read from Apache's listing). */
export const tomcatVersions = (line: string): Promise<string[]> =>
  bennu('bennu_tomcat_versions', { args: { line } });

/** The `context.xml` a configuration generates — the form's live preview. */
export const tomcatRenderContext = (root: string, config: RunConfigDto): Promise<string> =>
  bennu('bennu_tomcat_render_context', { args: { root, config } });

/** Store (or, empty, forget) a data source's password outside the repository. */
export const tomcatSetSecret = (
  root: string,
  configId: string,
  resourceId: string,
  value: string,
): Promise<void> =>
  bennu('bennu_tomcat_set_secret', { args: { root, config_id: configId, resource_id: resourceId, value } });

/** The ids of the resources that have a stored password — never the value. */
export const tomcatSecretIds = (root: string, configId: string): Promise<string[]> =>
  bennu('bennu_tomcat_secret_ids', { args: { root, config_id: configId } });

export type TomcatInstallEvent =
  | { kind: 'phase'; phase: 'lookup' | 'download' | 'verify' | 'extract' }
  | { kind: 'progress'; done: number; total: number | null }
  | { kind: 'done'; version: string; home: string }
  | { kind: 'error'; error: string };

export const EVT_TOMCAT_INSTALL = 'arbor://bennu/tomcat-install';

/** Starts the download and returns at once; follow it with {@link EVT_TOMCAT_INSTALL}. */
export const tomcatDownload = (root: string, version: string, installDir: string): Promise<void> =>
  bennu('bennu_tomcat_download', { args: { root, version, install_dir: installDir } });

/** Start Tomcat for `config`, streaming into the Run console. */
export const runTomcat = (
  root: string,
  config: RunConfigDto,
  opts: { env?: Record<string, string>; debug?: boolean } = {},
): Promise<RunHandle> =>
  bennu('bennu_run_tomcat', { args: { root, config, env: opts.env ?? null, debug: opts.debug ?? false } });
