/**
 * The "model suggestions" settings — the small local model that finishes the line you are typing, and the
 * log-statement completion that needs no model.
 *
 * Kept apart from the settings store for the reason `smart-completion.ts` is: it is a group that is always read,
 * written and reset together, and the mapping to and from the persisted config is pure, which makes it testable
 * without a Svelte runtime.
 */

import type { BennuConfig } from '$lib/ipc/bennu/config';

export interface FlcSettings {
  /** Draw the model's guess for the rest of the line as ghost text. A guess, so off until asked for. */
  enabled: boolean;
  /** The folder with the model files; empty = none, and then only the log suggestions work. */
  modelDir: string;
  /** How sure the model has to be, in percent, before its line is drawn. */
  minConfidence: number;
  /** Complete a log call from the pattern the project uses most in that place. Needs no model. */
  logs: boolean;
  alternatives: boolean;
  check: boolean;
  feedback: boolean;
  /** Learn from what happens to a ghost text after it is taken. Off until asked for. */
  learn: boolean;
  /** Leave blocks of commented-out code out of what the model reads, so it does not copy a version that was dropped. */
  ignoreCommentedCode: boolean;
  /** With `learn`: also keep the text itself, in a capped local log. */
  learnKeepText: boolean;
  /** With `learn`: a notification each time a correction is counted. For debugging; off. */
  learnNotify: boolean;
  /** Suggest the variable name after a type (`Order ▌` → `order`). */
  layerNames: boolean;
  /** Suggest the rest of a line the project most often wrote after the same words. */
  layerNgram: boolean;
  /** Suggest the arguments of a `new` the project always writes the same way. Off by default. */
  layerCreation: boolean;
  /** Suggest the call before the comma again, in a list that repeats itself. Off by default. */
  layerEcho: boolean;
  /** Suggest the usual arguments and variables of a call. */
  layerHabits: boolean;
  /** Remember what Bennu learned about each project on disk. */
  profileProject: boolean;
  /** Carry what it learned from one project to another. */
  profileGlobal: boolean;
}

export const MIN_CONFIDENCE = 30;
export const MAX_CONFIDENCE = 95;
export const CONFIDENCE_STEP = 5;

export const FLC_DEFAULTS: Readonly<FlcSettings> = {
  enabled: false,
  modelDir: '',
  minConfidence: 70,
  logs: true,
  alternatives: false,
  check: true,
  feedback: true,
  learn: false,
  ignoreCommentedCode: false,
  learnKeepText: false,
  learnNotify: false,
  layerNames: true,
  layerNgram: true,
  layerCreation: false,
  layerEcho: false,
  layerHabits: true,
  profileProject: true,
  profileGlobal: true,
};

/** The keys of the persisted config this group owns — what `flcToConfig` writes. */
export type FlcConfigFields = Pick<
  BennuConfig,
  'flc_enabled' | 'flc_model_dir' | 'flc_min_confidence' | 'flc_logs' | 'flc_alternatives' | 'flc_check' | 'flc_feedback' | 'flc_learn' | 'flc_ignore_commented_code' | 'flc_learn_keep_text' | 'flc_learn_notify' | 'flc_layer_names' | 'flc_layer_ngram' | 'flc_layer_creation' | 'flc_layer_echo' | 'flc_layer_habits' | 'flc_profile_project' | 'flc_profile_global'
>;

/** A threshold the backend would accept: a whole number inside the range the stepper offers. A hand-edited
 *  `flc_min_confidence = 0` must not turn every guess into ghost text. */
export function clampConfidence(value: number): number {
  if (!Number.isFinite(value)) return FLC_DEFAULTS.minConfidence;
  return Math.min(MAX_CONFIDENCE, Math.max(MIN_CONFIDENCE, Math.round(value)));
}

/** Read the group out of a config, filling whatever an older file does not carry with the default. */
export function flcFromConfig(cfg: Partial<FlcConfigFields>): FlcSettings {
  return {
    enabled: cfg.flc_enabled ?? FLC_DEFAULTS.enabled,
    modelDir: cfg.flc_model_dir ?? FLC_DEFAULTS.modelDir,
    minConfidence: clampConfidence(cfg.flc_min_confidence ?? FLC_DEFAULTS.minConfidence),
    logs: cfg.flc_logs ?? FLC_DEFAULTS.logs,
    alternatives: cfg.flc_alternatives ?? FLC_DEFAULTS.alternatives,
    check: cfg.flc_check ?? FLC_DEFAULTS.check,
    feedback: cfg.flc_feedback ?? FLC_DEFAULTS.feedback,
    learn: cfg.flc_learn ?? FLC_DEFAULTS.learn,
    ignoreCommentedCode: cfg.flc_ignore_commented_code ?? FLC_DEFAULTS.ignoreCommentedCode,
    learnKeepText: cfg.flc_learn_keep_text ?? FLC_DEFAULTS.learnKeepText,
    learnNotify: cfg.flc_learn_notify ?? FLC_DEFAULTS.learnNotify,
    layerNames: cfg.flc_layer_names ?? FLC_DEFAULTS.layerNames,
    layerNgram: cfg.flc_layer_ngram ?? FLC_DEFAULTS.layerNgram,
    layerCreation: cfg.flc_layer_creation ?? FLC_DEFAULTS.layerCreation,
    layerEcho: cfg.flc_layer_echo ?? FLC_DEFAULTS.layerEcho,
    layerHabits: cfg.flc_layer_habits ?? FLC_DEFAULTS.layerHabits,
    profileProject: cfg.flc_profile_project ?? FLC_DEFAULTS.profileProject,
    profileGlobal: cfg.flc_profile_global ?? FLC_DEFAULTS.profileGlobal,
  };
}

/** The config fields for `settings`, ready to spread into a `set_bennu_config` payload. */
export function flcToConfig(settings: FlcSettings): FlcConfigFields {
  return {
    flc_enabled: settings.enabled,
    flc_model_dir: settings.modelDir.trim(),
    flc_min_confidence: clampConfidence(settings.minConfidence),
    flc_logs: settings.logs,
    flc_alternatives: settings.alternatives,
    flc_check: settings.check,
    flc_feedback: settings.feedback,
    flc_learn: settings.learn,
    flc_ignore_commented_code: settings.ignoreCommentedCode,
    flc_learn_keep_text: settings.learn && settings.learnKeepText,
    flc_learn_notify: settings.learn && settings.learnNotify,
    flc_layer_names: settings.layerNames,
    flc_layer_ngram: settings.layerNgram,
    flc_layer_creation: settings.layerCreation,
    flc_layer_echo: settings.layerEcho,
    flc_layer_habits: settings.layerHabits,
    flc_profile_project: settings.profileProject,
    flc_profile_global: settings.profileGlobal,
  };
}

const ORIGIN_LABEL: Record<string, string> = { log: 'log statements', project: 'project lines', name: 'names', model: 'model' };
const ORIGIN_ORDER = ['log', 'project', 'name', 'model'];

/** One line per kind of suggestion that was shown at least once: `model: 41 shown, 12 taken (29%)`. Empty when none was. */
export function statsLines(origins: readonly { origin: string; shown: number; accepted: number; dismissed: number }[]): string[] {
  return [...origins]
    .filter((o) => o.shown > 0)
    .sort((a, b) => ORIGIN_ORDER.indexOf(a.origin) - ORIGIN_ORDER.indexOf(b.origin))
    .map((o) => `${ORIGIN_LABEL[o.origin] ?? o.origin}: ${o.shown} shown, ${o.accepted} taken (${Math.round((o.accepted / o.shown) * 100)}%)`);
}
