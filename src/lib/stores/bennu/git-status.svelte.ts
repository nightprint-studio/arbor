/** Bennu's instance of the shared git-status store (`$lib/stores/git-status.svelte`). */

import { createGitStatusStore } from '$lib/stores/git-status.svelte';

export const bennuGitStatusStore = createGitStatusStore();
