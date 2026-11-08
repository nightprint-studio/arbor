/**
 * Where every code editor's tooltips are mounted: one fixed layer on the document body, as wide as
 * the window and zero pixels tall.
 *
 * Not inside the editor — there a tooltip belongs to the editor's panel, whose `overflow: hidden` and
 * stacking order cut a long completion list at the panel's edge or put it under the console.
 *
 * ## Why exactly this shape — both dimensions have already broken once
 *
 * CodeMirror wraps an editor's tooltips in a container that is `position: relative` and carries the
 * editor's **theme classes**, so the theme's root rule — `height: 100%`, the editor background, the
 * base theme's flex column — applies to that container too.
 *
 * - **Height 0.** Appended to `document.body` directly, each container was a block as tall as the
 *   window, added to the layout after the app: opening the diffs pushed the whole app into the top
 *   half of the window. Inside a fixed, zero-height host the container is zero tall, takes no room,
 *   paints no background and covers no clicks.
 * - **Full width.** On WebKit (Tauri's macOS webview) CodeMirror's check for "is `fixed` working here"
 *   reads `rect.top + 10000` and trips as soon as a tooltip has been placed, so it switches the
 *   tooltips to `position: absolute` — **relative to that container**. A zero-width host made a
 *   zero-width container, and a shrink-to-fit tooltip in it wrapped after every letter. As wide as
 *   the window, the container is the same box the viewport is, and absolute placement against it is
 *   the fixed placement it replaces.
 *
 * No transform, filter or `contain` on the host: any of them would make it the containing block of
 * the tooltips inside. Its z-index is the app's tooltip layer, so a popup opened from an editor inside
 * a modal is drawn above the modal.
 */

const HOST_ID = 'cm-tooltip-host';

/** The shared tooltip layer, created on first use. */
export function tooltipHost(): HTMLElement {
  const existing = document.getElementById(HOST_ID);
  if (existing) return existing;
  const host = document.createElement('div');
  host.id = HOST_ID;
  host.style.cssText =
    'position: fixed; top: 0; left: 0; right: 0; height: 0; overflow: visible; z-index: var(--z-tooltip);';
  document.body.appendChild(host);
  return host;
}
