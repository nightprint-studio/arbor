import { cubicOut } from 'svelte/easing';
import type { TransitionConfig } from 'svelte/transition';

/**
 * IDE-style panel open/close transitions, shared by the main window (AppShell)
 * and the nemus window (WorkspaceShell / PanelCard). They animate the box
 * *collapsing* rather than fading so docked panels behave like JetBrains tool
 * windows — the neighbours reflow smoothly as the panel grows/shrinks.
 *
 * Each is a Svelte transition fn: apply with `transition:sidebarSlide={{ duration }}`
 * on the element that mounts/unmounts inside an `{#if}`.
 */

interface SlideParams {
  duration?: number;
}

interface SidebarSlideParams extends SlideParams {
  /** Animate a trailing gap (margin-right, px) alongside the width — for
   *  panels separated from their neighbour by a fixed gap. Default 0. */
  gap?: number;
  /** Fade opacity together with the width. Default false. */
  fade?: boolean;
  /** Pin `flex: 0 0 auto` during the slide so a flex sibling doesn't reflow
   *  the collapsing panel mid-transition. Default false. */
  rigid?: boolean;
}

/**
 * Collapse/expand width — left/right sidebars. Defaults match the docked-card
 * use (PanelCard): plain width collapse. The opt-in `gap`/`fade`/`rigid` flags
 * cover the in-modal sidebars (conflict resolution, worktree-link manager),
 * which fade + leave a gap while a flex sibling rearranges.
 */
export function sidebarSlide(
  node: HTMLElement,
  { duration = 200, gap = 0, fade = false, rigid = false }: SidebarSlideParams = {},
): TransitionConfig {
  const w = node.getBoundingClientRect().width;
  return {
    duration,
    easing: cubicOut,
    css: (t: number) =>
      `width: ${t * w}px; min-width: 0; overflow: hidden;`
      + (gap ? ` margin-right: ${t * gap}px;` : '')
      + (fade ? ` opacity: ${t};` : '')
      + (rigid ? ' flex: 0 0 auto;' : ''),
  };
}

/** Collapse/expand height — bottom docked panel. */
export function bottomSlide(
  node: HTMLElement,
  { duration = 200 }: SlideParams = {},
): TransitionConfig {
  const h = node.getBoundingClientRect().height;
  return {
    duration,
    easing: cubicOut,
    css: (t: number) => `height: ${t * h}px; min-height: 0; overflow: hidden;`,
  };
}

/**
 * Collapse/expand width for the 38-px activity-bar rail. Same shape as
 * `sidebarSlide` but falls back to 38 when the rail measures 0 (it can be
 * mounted at width 0 in the same frame it's added), so the slide-in always
 * has a target width.
 */
export function barSlide(
  node: HTMLElement,
  { duration = 200 }: SlideParams = {},
): TransitionConfig {
  const w = node.getBoundingClientRect().width || 38;
  return {
    duration,
    easing: cubicOut,
    css: (t: number) => `width: ${t * w}px; min-width: 0; overflow: hidden;`,
  };
}
