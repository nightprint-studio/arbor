import { invoke } from '@tauri-apps/api/core';
import type {
  PluginContribution, ContributionPoint,
} from '$lib/types/contribution';

export async function listPluginContributions(point?: string): Promise<PluginContribution[]> {
  return invoke<PluginContribution[]>('list_plugin_contributions', { point: point ?? null });
}

export async function listContributionPoints(): Promise<ContributionPoint[]> {
  return invoke<ContributionPoint[]>('list_contribution_points');
}
