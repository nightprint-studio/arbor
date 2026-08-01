// Stage the product backend binaries into `src-tauri/backends/` so Tauri bundles
// them (as resources) into a dedicated `backends/` subfolder of the installed
// app. Run after building the backends in release; before `tauri build`.
//
// Dev doesn't need this: `cargo build -p <name>` co-locates the binary beside the
// launcher in `target/debug/`, where the launcher's resolver finds it directly.
//
// Node built-ins only — no dependencies.

import { mkdirSync, copyFileSync, existsSync } from 'node:fs';

const EXE = process.platform === 'win32' ? '.exe' : '';
// Being in `backends:release` is NOT enough: that only *builds* a backend. A
// binary missing from this list is never copied into `src-tauri/backends/`, which
// is what `tauri.conf.json`'s `resources` bundles — so it works in dev (the
// launcher finds it beside itself in `target/debug/`) and silently has no backend
// in an installed app. That failure is invisible until someone installs a build.
//
// NOTE: `tyto-be` and `bennu-be` are still in that state. Left alone here because
// which binaries ship is a release call, not a side effect — but it looks like an
// oversight rather than a decision, and it is worth confirming.
const BACKENDS = ['corvus-be', 'merula-be', 'sitta-be', 'picus-be', 'garrulus-be'];
const destDir = 'src-tauri/backends';

mkdirSync(destDir, { recursive: true });

for (const name of BACKENDS) {
  const src = `src-tauri/target/release/${name}${EXE}`;
  if (!existsSync(src)) {
    console.error(`stage-backends: ${src} not found — run "npm run backends:release" first`);
    process.exit(1);
  }
  copyFileSync(src, `${destDir}/${name}${EXE}`);
  console.log(`stage-backends: ${name}${EXE} → ${destDir}/`);
}
