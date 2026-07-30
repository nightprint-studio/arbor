/**
 * Picus's right-click menu — one instance, mounted by `PicusShell`.
 *
 * See `createContextMenuStore` for the shape and for why this is an instance
 * rather than a singleton.
 */

import { createContextMenuStore } from '../contextmenu.svelte';

export const picusContextMenuStore = createContextMenuStore();
