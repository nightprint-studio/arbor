// Dev-only Vite plugin for the UI gallery: lets the page POST what a scene rendered to a file,
// so two revisions of the app can be dumped side by side and diffed (see scripts/gallery-diff.mjs).
// Only `vite.gallery.config.js` loads it; the normal config and production builds never see it.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export function galleryDump() {
  const outDir = process.env.GALLERY_OUT ?? '.gallery-out';
  return {
    name: 'arbor-gallery-dump',
    apply: 'serve',
    configureServer(server) {
      mkdirSync(outDir, { recursive: true });
      server.middlewares.use('/__dump', (req, res) => {
        if (req.method !== 'POST') { res.statusCode = 405; res.end(); return; }
        const chunks = [];
        req.on('data', (c) => chunks.push(c));
        req.on('end', () => {
          try {
            const { id, result } = JSON.parse(Buffer.concat(chunks).toString('utf8'));
            // `corvus/CloneRepoModal` → `corvus__CloneRepoModal.json`; refuse anything path-like.
            const file = String(id).replace(/[^A-Za-z0-9_-]+/g, '__') + '.json';
            writeFileSync(join(outDir, file), JSON.stringify(result));
            res.end('ok');
          } catch (e) {
            res.statusCode = 400;
            res.end(String(e));
          }
        });
      });
    },
  };
}
