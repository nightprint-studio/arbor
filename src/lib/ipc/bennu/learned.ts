/**
 * What Bennu has learned, as the backend reads it out — and the calls that take a row back.
 *
 * Wire: `bennu_learned_report`, `bennu_learned_hide`, `bennu_learned_restore`, `bennu_learned_forget_correction`.
 */

import { bennu } from '../rpc';

export interface LearnedItem {
  text: string;
  count: number;
  /** The share of the place's sightings this row holds, 0..1. */
  share: number | null;
  /** The user asked for it not to be offered. */
  hidden: boolean;
  /** What else is known about the row (for corrections: what became of the suggestions). */
  detail: string | null;
}

export interface LearnedGroup {
  key: string;
  total: number;
  /** The whole place is hidden: nothing is offered from it. */
  hidden: boolean;
  items: LearnedItem[];
}

export interface LearnedSection {
  id: string;
  /** The kind a rule is made with; empty where a row can only be forgotten outright. */
  kind: string;
  title: string;
  about: string;
  groups: LearnedGroup[];
}

export interface LearnedReport {
  sections: LearnedSection[];
  /** How the project lays out its code, one line per thing it is consistent about. */
  layout: string[];
  learning_on: boolean;
}

export function learnedReport(): Promise<LearnedReport> {
  return bennu('bennu_learned_report', {});
}

export interface LearnedRule {
  kind: string;
  key: string;
  text: string;
}

export function learnedHide(rule: LearnedRule): Promise<void> {
  return bennu('bennu_learned_hide', { args: rule });
}

export function learnedRestore(rule: LearnedRule): Promise<void> {
  return bennu('bennu_learned_restore', { args: rule });
}

export function learnedForgetCorrection(origin: string, context: string): Promise<void> {
  return bennu('bennu_learned_forget_correction', { args: { origin, context } });
}
