/**
 * The "smart suggestions" settings — which completion signals are on, and how strongly the learned
 * ones count.
 *
 * Kept apart from the settings store because it is a self-contained group (eight fields that are
 * always read, written and reset together) and because the mapping to and from the persisted
 * config is pure — which makes it testable without a Svelte runtime. Each switch turns a *signal*
 * off, never a candidate: nothing here filters the list.
 */

import type { BennuConfig } from '$lib/ipc/bennu/config';

export interface SmartCompletionSettings {
  /** Rank a candidate higher when its name echoes the name of the thing being filled. */
  nameAffinity: boolean;
  /** Rank a member higher when the project calls it often. */
  projectUsage: boolean;
  /** Rank a member higher when the project calls it right after the previous call in the chain. */
  sequence: boolean;
  /** Also offer members of members that produce the expected type. */
  chain: boolean;
  /** Let the shape of the code decide: builder repeats sink, `catch`/`case` lead with what fits. */
  context: boolean;
  /** Offer whole-statement templates that read the surrounding code. */
  statements: boolean;
  /** Draw the continuations that are certain as ghost text. */
  ghostText: boolean;
  /** How strongly the learned signals count, in percent of their designed weight. */
  strength: number;
}

export const MIN_STRENGTH = 0;
export const MAX_STRENGTH = 250;
export const STRENGTH_STEP = 25;

export const SMART_DEFAULTS: Readonly<SmartCompletionSettings> = {
  nameAffinity: true,
  projectUsage: true,
  sequence: true,
  chain: true,
  context: true,
  statements: true,
  ghostText: true,
  strength: 100,
};

/** The keys of the persisted config this group owns — what `smartToConfig` writes. */
export type SmartConfigFields = Pick<
  BennuConfig,
  | 'completion_smart_name_affinity'
  | 'completion_smart_project_usage'
  | 'completion_smart_sequence'
  | 'completion_smart_chain'
  | 'completion_smart_context'
  | 'completion_smart_statements'
  | 'completion_smart_ghost_text'
  | 'completion_smart_strength'
>;

/** A strength the backend would accept: a whole number inside the range the stepper offers. A
 *  hand-edited `completion_smart_strength = 9000` must not leave a ranking nobody can reason about. */
export function clampStrength(value: number): number {
  if (!Number.isFinite(value)) return SMART_DEFAULTS.strength;
  return Math.min(MAX_STRENGTH, Math.max(MIN_STRENGTH, Math.round(value)));
}

/** Read the group out of a config, filling whatever an older file does not carry with the default. */
export function smartFromConfig(cfg: Partial<SmartConfigFields>): SmartCompletionSettings {
  return {
    nameAffinity: cfg.completion_smart_name_affinity ?? SMART_DEFAULTS.nameAffinity,
    projectUsage: cfg.completion_smart_project_usage ?? SMART_DEFAULTS.projectUsage,
    sequence: cfg.completion_smart_sequence ?? SMART_DEFAULTS.sequence,
    chain: cfg.completion_smart_chain ?? SMART_DEFAULTS.chain,
    context: cfg.completion_smart_context ?? SMART_DEFAULTS.context,
    statements: cfg.completion_smart_statements ?? SMART_DEFAULTS.statements,
    ghostText: cfg.completion_smart_ghost_text ?? SMART_DEFAULTS.ghostText,
    strength: clampStrength(cfg.completion_smart_strength ?? SMART_DEFAULTS.strength),
  };
}

/** The config fields for `settings`, ready to spread into a `set_bennu_config` payload. */
export function smartToConfig(settings: SmartCompletionSettings): SmartConfigFields {
  return {
    completion_smart_name_affinity: settings.nameAffinity,
    completion_smart_project_usage: settings.projectUsage,
    completion_smart_sequence: settings.sequence,
    completion_smart_chain: settings.chain,
    completion_smart_context: settings.context,
    completion_smart_statements: settings.statements,
    completion_smart_ghost_text: settings.ghostText,
    completion_smart_strength: clampStrength(settings.strength),
  };
}
