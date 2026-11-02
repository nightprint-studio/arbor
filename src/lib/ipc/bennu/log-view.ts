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

/** The top-level packages the project's own sources declare, most populated first. */
export const ownPackages = (root: string): Promise<string[]> => bennu('bennu_own_packages', { root });
