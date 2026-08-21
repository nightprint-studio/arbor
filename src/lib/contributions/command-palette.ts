/**
 * Pure mapping `PluginContribution → PluginPaletteCommand`.
 *
 * `arbor.command.register` is how a plugin makes an action findable by name, and every
 * product's palette has to offer the same set — a plugin that registers a command in Bennu
 * gets it in Bennu's palette, not in Corvus's. The two palettes render differently (Corvus
 * scores and slices, Bennu filters on a substring), so what is shared is the *reading* of the
 * contribution and the *firing* of the command, not the presentation.
 *
 * Without this the palettes each parse the payload themselves, and the second one to be
 * written simply forgets — which is exactly what happened: Bennu ran the plugins, the
 * commands registered, and nothing anywhere could invoke them.
 */
import { contributionStore } from '$lib/stores/corvus/contribution.svelte';
import { firePluginAction } from '$lib/ipc/plugin';

export const COMMAND_PALETTE_POINT = 'arbor:command-palette';

interface CommandPayload {
  title?:       string;
  description?: string;
  icon?:        string;
}

export interface PluginPaletteCommand {
  /** Unique across plugins — safe as a palette row key. */
  id:          string;
  pluginName:  string;
  title:       string;
  /** Falls back to the plugin's name, so a row is never subtitle-less. */
  subtitle:    string;
  icon:        string;
  /** Everything the palette needs to match a typed query against. */
  haystack:    string;
  /** Fire the command on its owning plugin. Fire-and-forget by design: the plugin
   *  decides what happens next, and the palette closes either way. */
  run:         () => void;
}

export function pluginPaletteCommands(): PluginPaletteCommand[] {
  return contributionStore.forPoint(COMMAND_PALETTE_POINT).map((c) => {
    const p = c.payload as CommandPayload;
    const title    = p.title ?? c.item_id;
    const subtitle = p.description ?? c.plugin_name;
    return {
      id:         `plugin:${c.plugin_name}:${c.item_id}`,
      pluginName: c.plugin_name,
      title,
      subtitle,
      icon:       p.icon ?? 'Zap',
      haystack:   `${title} ${p.description ?? ''} ${c.plugin_name}`,
      run: () => {
        // Logged, not swallowed. A palette entry that fails silently is indistinguishable
        // from one that did nothing, and that is exactly the bug this file exists to fix.
        firePluginAction(c.plugin_name, `command:${c.item_id}`, '{}')
          .catch((e) => console.error(`plugin '${c.plugin_name}': command '${c.item_id}' failed`, e));
      },
    };
  });
}
