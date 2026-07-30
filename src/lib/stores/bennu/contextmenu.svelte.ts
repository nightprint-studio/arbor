/**
 * Bennu's right-click menu.
 *
 * The behaviour is `createContextMenuStore` — shared with every other product,
 * because "where the menu is, what is in it, and who gets told what was picked"
 * was never a Bennu idea. What is Bennu's is the *instance*: one per window, so a
 * right-click in the Java editor cannot open a menu belonging to another product
 * mounted in the same WebView.
 */

import { createContextMenuStore } from '../contextmenu.svelte';

export const bennuContextMenuStore = createContextMenuStore();
