import { browser, dev } from '$app/environment';

// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Dev-only: `?mock[&label=bennu]` swaps the Tauri bridge for fixtures so a screen can be
// opened in a plain browser (screenshots, visual regression). `dev` is a build-time
// constant, so the branch and the dynamic import are dropped from production bundles.
export const load = async () => {
  if (dev && browser) {
    const q = new URLSearchParams(location.search);
    if (q.has('mock')) {
      const { installTauriMock } = await import('$lib/dev/tauri-mock');
      installTauriMock(q.get('label') ?? 'main');
    }
  }
};
