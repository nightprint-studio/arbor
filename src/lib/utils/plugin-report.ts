/**
 * One way for the frontend to say **"this plugin failed"**.
 *
 * The Rust twin of this file is `arbor-plugin-core`'s `PluginReporter`, and it
 * exists for the same reason: a plugin failure has two audiences. The browser
 * console is read while devtools happen to be open; the **Plugin Logs panel** is
 * read by whoever is writing the plugin, from inside the app, usually after the
 * fact. A site that writes to only one of the two is silent for the audience
 * that needed it — and for a released build, that audience is the only one.
 *
 * The frontend half matters more than it looks. Half of what a plugin does
 * happens in the webview: an action it fired, a form node it described, a tree
 * payload it sent. Those used to fail into `.catch(() => {})`, which is the
 * quietest possible failure — nothing in the panel, nothing in the console,
 * nothing anywhere.
 *
 * Callers do not need to reach for this by hand for plugin *actions*:
 * `firePluginAction` / `fireCommand` / `execHook` in `$lib/ipc/plugin` report
 * their own rejections. Use it directly for the failures that never became an
 * IPC call — a payload that would not render, an id that resolves to nothing.
 */

import { recordPluginLog } from '$lib/ipc/plugin-logs';
import type { PluginLogLevel } from '$lib/types/plugin-logs';

/** Whatever was thrown, as a sentence. `Error` keeps its message; anything else
 *  is stringified, because a plugin's rejection comes across the IPC boundary as
 *  a plain string and wrapping it in `[object Object]` helps no one. */
export function describeError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  try { return JSON.stringify(err); } catch { return String(err); }
}

function say(level: PluginLogLevel, plugin: string, message: string) {
  const line = `[plugin:${plugin}] ${message}`;
  if (level === 'error') console.error(line);
  else if (level === 'warn') console.warn(line);
  else console.info(line);
  // Fire-and-forget: reporting a failure must not become one. The panel losing
  // an entry is worse than the console losing it, but neither is worth throwing
  // out of a catch block that was already handling something else.
  recordPluginLog(level, plugin, message).catch(() => {});
}

/** Something the plugin asked for did not happen. */
export function reportPluginError(plugin: string, message: string, err?: unknown) {
  say('error', plugin, err === undefined ? message : `${message}: ${describeError(err)}`);
}

/** It happened, but not the way the plugin meant it to — a shape that had to be
 *  coerced, an id that matched nothing, a name that collided. */
export function reportPluginWarning(plugin: string, message: string, err?: unknown) {
  say('warn', plugin, err === undefined ? message : `${message}: ${describeError(err)}`);
}
