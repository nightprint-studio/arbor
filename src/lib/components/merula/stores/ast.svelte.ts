/** The syntax tree of the active `.merula` buffer — the shared store over merula's grammar. */

import { syntaxPathAt, syntaxTreeOf } from '$lib/ipc/merula/ast';
import { createSyntaxTreeStore } from '$lib/stores/syntax-tree.svelte';

export const astStore = createSyntaxTreeStore({ treeOf: syntaxTreeOf, pathAt: syntaxPathAt });
