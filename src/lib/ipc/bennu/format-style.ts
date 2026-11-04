/**
 * The Java formatter's style — settings in all their layers, the live preview and the project
 * detection. The formatting itself (`formatBuffer`) lives in `format.ts`.
 *
 * Same conventions as the rest of `ipc/bennu`: every call wraps its fields under `{ args: … }`,
 * snake_case.
 */

import { bennu } from '../rpc';
import type { FormatDetection, FormatPatch, FormatSettings, FormatStyle } from '$lib/types/bennu-format';

/**
 * Every layer of the style, for the project at `root` (the global ones alone when omitted).
 *
 * Wire: `bennu_format_settings` — `{ root? }`.
 */
export function formatSettings(root?: string): Promise<FormatSettings> {
  return bennu('bennu_format_settings', { args: { root } });
}

/** Save the global style (a full style). Wire: `bennu_format_set_global` — `{ style }`. */
export function formatSetGlobal(style: FormatStyle): Promise<void> {
  return bennu('bennu_format_set_global', { args: { style } });
}

/**
 * Save the project's sparse override into `<root>/.arbor/bennu/config.toml [format]`; `null`
 * removes it. Wire: `bennu_format_set_project` — `{ root, overrides }`.
 */
export function formatSetProject(root: string, overrides: FormatPatch | null): Promise<void> {
  return bennu('bennu_format_set_project', { args: { root, overrides } });
}

/** `source` formatted with `style` — pure and fast. Wire: `bennu_format_preview` — `{ style, source }`. */
export function formatPreview(style: FormatStyle, source: string): Promise<string> {
  return bennu('bennu_format_preview', { args: { style, source } });
}

/**
 * Measure the project's existing code. May take a while on a big tree.
 * Wire: `bennu_format_detect` — `{ root }`.
 */
export function formatDetect(root: string): Promise<FormatDetection> {
  return bennu('bennu_format_detect', { args: { root } });
}
