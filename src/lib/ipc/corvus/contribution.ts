import { corvus } from '../rpc';
import type {
  PluginContribution, ContributionPoint,
} from '$lib/types/corvus/contribution';

export async function listPluginContributions(point?: string): Promise<PluginContribution[]> {
  return corvus<PluginContribution[]>('list_plugin_contributions', { point: point ?? null });
}

export async function listContributionPoints(): Promise<ContributionPoint[]> {
  return corvus<ContributionPoint[]>('list_contribution_points');
}
