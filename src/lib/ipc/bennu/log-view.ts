/**
 * Console "clean view" IPC — the saved settings, and which packages are the project's own.
 *
 * Wire shapes mirror `LogViewConfig` in `crates/products/bennu/runtime/core/src/config.rs`; the
 * handlers live in `crates/products/bennu/runtime/be/src/log_view.rs`.
 */

import { bennu } from '../rpc';
import type { LogViewSettings } from '$lib/components/bennu/console/log-view';

export const getLogView = (): Promise<LogViewSettings> => bennu('get_bennu_log_view', {});

export const setLogView = (config: LogViewSettings): Promise<void> => bennu('set_bennu_log_view', { config });

/** A logging configuration of the project that names a console pattern. */
export interface LogLayoutSource {
  /** Relative to the project root. */
  file: string;
  /** `logback` | `log4j2` | `log4j` | `spring`. */
  kind: string;
  pattern: string;
  /** The console can read its columns with it. */
  usable: boolean;
}

/** The project's logging configurations, in the order they are consulted. */
export const logLayouts = (root: string): Promise<LogLayoutSource[]> => bennu('bennu_log_layouts', { root });

/** The top-level packages the project's own sources declare, most populated first. */
export const ownPackages = (root: string): Promise<string[]> => bennu('bennu_own_packages', { root });

/** A log file the project has on disk. */
export interface LogFileInfo {
  /** Absolute. */
  path: string;
  name: string;
  /** Relative to the project root. */
  rel: string;
  /** `tomcat` | `project`. */
  group: 'tomcat' | 'project';
  size: number;
  /** Milliseconds since the epoch. */
  modified: number;
}

/** The log files the project has: Tomcat's, loose `*.log`, `logs/` folders, and what its logging configuration writes to. */
export const logFiles = (root: string): Promise<LogFileInfo[]> => bennu('bennu_log_files', { root });

/** Start following a file; its lines arrive as `arbor://bennu/log-tail` events carrying the returned id. */
export const logTailStart = (root: string, path: string): Promise<string> => bennu('bennu_log_tail_start', { root, path });

export const logTailStop = (id: string): Promise<void> => bennu('bennu_log_tail_stop', { id });
