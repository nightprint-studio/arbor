// Vite config for the dev UI gallery: the app's own config plus the dump endpoint.
// Run with `npx vite dev --config vite.gallery.config.js --port <port>`; set GALLERY_OUT to
// choose where snapshots land. Not used by `tauri dev` or any build.
import { realpathSync } from 'node:fs';
import { defineConfig } from 'vite';
import base from './vite.config.js';
import { galleryDump } from './scripts/gallery-dump-plugin.mjs';

export default defineConfig(async (env) => {
  const cfg = await (typeof base === 'function' ? base(env) : base);
  return {
    ...cfg,
    // Two galleries (e.g. working tree vs a checkout of HEAD) can share one node_modules;
    // without a private dependency cache each re-optimises under the other and the page loads
    // two copies of Svelte ("effect_orphan"). Give every instance its own via GALLERY_CACHE.
    cacheDir: process.env.GALLERY_CACHE ?? 'node_modules/.vite-gallery',
    // A checkout whose node_modules is a symlink to another tree resolves packages outside
    // its own root; Vite refuses to serve those unless the real location is allowed.
    server: { ...cfg.server, fs: { allow: [process.cwd(), realpathSync('node_modules')] } },
    plugins: [...(cfg.plugins ?? []), galleryDump()],
  };
});
