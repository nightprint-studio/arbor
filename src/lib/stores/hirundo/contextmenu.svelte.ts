/**
 * The right-click menu of the Hirundo window. The behaviour is `createContextMenuStore` — shared with every
 * other product; this is only Hirundo's instance (one per window, see the factory for why).
 */
import { createContextMenuStore } from '../contextmenu.svelte';

export const hirundoContextMenuStore = createContextMenuStore();
