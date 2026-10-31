/**
 * The transition every expand/collapse uses: height and opacity together, over the app's panel duration, so a block that
 * opens does not jump the rows under it. One definition because a dozen hand-made `{#if open}` blocks had none, and the ones
 * that did each picked their own duration and easing.
 *
 * `<div transition:collapse>…</div>` on the element that appears and goes. The element needs `overflow: hidden` only if its
 * content would otherwise spill while it grows — `slide` sets it for the length of the animation.
 */
import { slide, type SlideParams } from 'svelte/transition';
import { cubicOut } from 'svelte/easing';
import { animStore } from '$lib/stores/animations.svelte';

export function collapse(node: Element, params: SlideParams = {}) {
  return slide(node, { duration: animStore.dPanel, easing: cubicOut, ...params });
}
