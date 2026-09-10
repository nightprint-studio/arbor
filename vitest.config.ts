import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

/**
 * Unit tests for the parts of the frontend that are **logic**, not markup.
 *
 * Deliberately narrow: this runs plain `.ts` modules in Node, with no DOM and no Svelte compiler.
 * What earns a test here is code whose failures are about ORDER or STATE — the navigation flow is
 * the first — because those are the ones that are impossible to see in a component and impossible
 * to reproduce by hand. Rendering is still checked by `svelte-check` and by looking at it.
 */
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
  // The one SvelteKit alias these modules use. Without it a module under test cannot import a
  // sibling the ordinary way, and the test would be running a hand-copied variant of the code
  // rather than the code.
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
    },
  },
});
