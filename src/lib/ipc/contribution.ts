import { platform } from './rpc';
import type {
  PluginContribution, ContributionPoint,
} from '$lib/types/contribution';

export async function listPluginContributions(point?: string): Promise<PluginContribution[]> {
  return platform<PluginContribution[]>('list_plugin_contributions', { point: point ?? null });
}

export async function listContributionPoints(): Promise<ContributionPoint[]> {
  return platform<ContributionPoint[]>('list_contribution_points');
}
