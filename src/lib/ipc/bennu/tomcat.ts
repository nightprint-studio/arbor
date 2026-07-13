/**
 * Bennu Tomcat hot-swap IPC — link a project to a local Tomcat and copy changed JSPs into the
 * deployed (exploded) webapp so Jasper recompiles them on next request. Routes through the generic
 * `bennu(...)` rpc bridge; wire shapes mirror the BE handlers in
 * `crates/products/bennu/be/src/tomcat.rs` verbatim (snake_case fields under `{ args: … }`).
 */

import { bennu } from '../rpc';

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
