// Pure helpers for the pipeline editor sub-components — extracted during the
// Phase 4 god-object refactor. See project_god_objects_refactor.md.
//
// These are dependency-free utilities (icons, colours, tree-walking, action
// dispatch) reused by Palette / Sequence / Detail and by the dispatcher.

import {
  HelpCircle, Circle,
  AlignHorizontalSpaceBetween, AlignVerticalSpaceBetween,
} from 'lucide-svelte';

import type { Step, Branch, FireAction, FireKey } from './types';

// Subtle per-category accent colours for the op icons (palette + step list).
// Tones are desaturated on purpose: the colour is a semantic cue, not a
// decoration — bright hues fight the JetBrains-dark theme. Each entry is a
// fallback if the category isn't recognised the icon keeps the muted default.
export const CAT_COLOR: Record<string, string> = {
  file:       '#7aa7d9',   // blue
  content:    '#b393d9',   // muted lavender
  git:        '#d9a36b',   // amber
  build:      '#7fc1a7',   // teal
  validation: '#98c079',   // sage green
  execution:  '#9aa4b4',   // slate
  flow:       '#d18fb0',   // dusty pink
};

export function catColor(cat: string | undefined): string {
  return (cat && CAT_COLOR[cat]) || 'var(--text-muted)';
}

// Icons used by the editor chrome itself (not by op definitions). Lookup falls
// through to the plugin-shipped iconMap first, then this local set, then a
// neutral Circle placeholder.
export const LOCAL_ICONS: Record<string, any> = {
  HelpCircle, Circle,
  AlignHorizontalSpaceBetween, AlignVerticalSpaceBetween,
};

export function iconFor(
  iconMap: Record<string, any> | undefined,
  name: string | undefined,
) {
  if (!name) return Circle;
  return iconMap?.[name] ?? LOCAL_ICONS[name] ?? Circle;
}

/** Walk a step subtree and apply `cb` to every branch (if + elif + else). */
export function forEachBranch(
  steps: Step[] | undefined,
  cb: (b: Branch) => void,
) {
  if (!Array.isArray(steps)) return;
  for (const s of steps) {
    if (Array.isArray(s.branches)) {
      for (const b of s.branches) {
        cb(b);
        forEachBranch(b.steps, cb);
      }
    }
    if (s.else_branch) {
      cb(s.else_branch);
      forEachBranch(s.else_branch.steps, cb);
    }
  }
}

/** Build a `fire(actionKey, extra)` helper closed over the plugin's actions
 * hash. Returns a no-op when the requested key is missing — same semantics as
 * the pre-refactor inline helper. */
export function makeFire(
  actions: Record<string, string> | undefined,
  fireAction: FireAction,
): FireKey {
  return (actionKey, extra) => {
    const action = actions?.[actionKey];
    if (!action) return;
    fireAction(action, extra ?? {});
  };
}
