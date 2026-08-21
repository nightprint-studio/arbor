/**
 * The cross-plugin contribution registry.
 *
 * Routed through `host()`, not `corvus()`: contributions belong to whichever backend is
 * running the plugins that declared them, and since Bennu grew a plugin host that is no
 * longer always Corvus. Asking Corvus from a Bennu window returns Corvus's contributions —
 * or nothing, when corvus-be is not running — which reads as "this plugin contributed
 * nothing" rather than as a question sent to the wrong place.
 */
import { host } from '../host';
import type {
  PluginContribution, ContributionPoint,
} from '$lib/types/corvus/contribution';

export async function listPluginContributions(point?: string): Promise<PluginContribution[]> {
  return host<PluginContribution[]>('list_plugin_contributions', { point: point ?? null });
}

export async function listContributionPoints(): Promise<ContributionPoint[]> {
  return host<ContributionPoint[]>('list_contribution_points');
}
