/**
 * The syntax tree of whatever Picus document is in front of the user — the shared
 * syntax-tree store (`$lib/stores/syntax-tree.svelte`) pointed at Picus's SQL grammar.
 */

import { syntaxPathAt, syntaxTreeOf } from '$lib/ipc/picus/ast';
import { createSyntaxTreeStore } from '$lib/stores/syntax-tree.svelte';

export const astStore = createSyntaxTreeStore({ treeOf: syntaxTreeOf, pathAt: syntaxPathAt });
