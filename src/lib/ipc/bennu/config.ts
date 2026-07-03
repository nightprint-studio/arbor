/**
 * Bennu product-config IPC — the typed per-profile `…/bennu/config.toml` the editor
 * persists (default encoding, indent width, extra JDK search paths, per-project JDK /
 * encoding overrides). Round-trips through the generic `bennu(...)` rpc bridge to the
 * `get_bennu_config` / `set_bennu_config` handlers in `bennu-be`.
 *
 * Kept in its own file so concurrent edits to the main bennu IPC surface don't race.
 */

import { bennu } from '../rpc';

/** Mirrors the BE `BennuConfig` (snake_case, field-for-field). */
export interface BennuConfig {
  /** Fallback text encoding when a project declares none. */
  default_encoding: string;
  /** Editor indentation width in spaces. */
  indent_width: number;
  /** Extra JDK install directories to search, on top of `JAVA_HOME` + the standard roots. */
  jdk_paths: string[];
  /** Per-project JDK override, keyed by absolute project-root path → Java version. */
  jdk_overrides: Record<string, string>;
  /** Per-project / per-file encoding override, keyed by absolute path → encoding label. */
  encoding_overrides: Record<string, string>;
}

/** Read the typed bennu config (BE returns defaults on a missing/corrupt file). */
export function getBennuConfig(): Promise<BennuConfig> {
  return bennu('get_bennu_config', {});
}

/** Persist the typed bennu config. The BE also re-seeds the classpath's extra JDK search
 *  dirs from `jdk_paths` so a change takes effect on the next index build. */
export function setBennuConfig(config: BennuConfig): Promise<void> {
  return bennu('set_bennu_config', { config });
}
